use super::{
    entries::ItemId,
    pane::{Generation, PaneId},
    pe_icon::windows_executable_icon_png,
};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

pub mod scheduler;

pub use scheduler::{
    ThumbnailCandidate, ThumbnailProbeBatch, ThumbnailProbeCancelHandle, ThumbnailProbeResult,
    ThumbnailScheduler, ThumbnailWorkKey, apply_thumbnail_probe_result_to_model,
    thumbnail_candidate_failure_is_cached, thumbnail_probe_results_for_requests,
};

const THUMBNAILS_DIR: &str = "thumbnails";
const NORMAL_DIR: &str = "normal";
const LARGE_DIR: &str = "large";
const FAIL_DIR: &str = "fail";
const FAIL_APP_DIR: &str = "gnome-thumbnail-factory";
const PNG_EXTENSION: &str = "png";
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const PNG_CHUNK_HEADER_LEN: usize = 8;
const PNG_CHUNK_CRC_LEN: usize = 4;

const FAILURE_THUMBNAIL_IDAT: &[u8] = &[0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailSize {
    Normal,
    Large,
}

impl ThumbnailSize {
    pub fn cache_dir(self) -> &'static str {
        match self {
            Self::Normal => NORMAL_DIR,
            Self::Large => LARGE_DIR,
        }
    }

    pub fn max_dimension(self) -> u16 {
        match self {
            Self::Normal => 128,
            Self::Large => 256,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCacheHit {
    size: ThumbnailSize,
    path: PathBuf,
}

impl ThumbnailCacheHit {
    pub fn size(&self) -> ThumbnailSize {
        self.size
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCachePaths {
    pub normal: PathBuf,
    pub large: PathBuf,
    pub failure: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalThumbnailerCommand {
    program: String,
    args: Vec<OsString>,
}

impl ExternalThumbnailerCommand {
    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[OsString] {
        &self.args
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThumbnailMetadata {
    pub uri: Option<String>,
    pub mtime: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailRequestPriority {
    Visible,
    Deferred,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailRequest {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    path: PathBuf,
    uri: String,
    modified_secs: u64,
    mime_type: Option<String>,
    priority: ThumbnailRequestPriority,
}

impl ThumbnailRequest {
    pub fn new(
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        priority: ThumbnailRequestPriority,
    ) -> Option<Self> {
        let modified_secs = file_modified_secs(&path)?;
        Self::from_entry_metadata(pane_id, generation, item_id, path, modified_secs, priority)
    }

    pub fn from_entry_metadata(
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        modified_secs: u64,
        priority: ThumbnailRequestPriority,
    ) -> Option<Self> {
        Self::from_entry_metadata_with_mime(
            pane_id,
            generation,
            item_id,
            path,
            modified_secs,
            None,
            priority,
        )
    }

    pub fn from_entry_metadata_with_mime(
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        modified_secs: u64,
        mime_type: Option<String>,
        priority: ThumbnailRequestPriority,
    ) -> Option<Self> {
        let uri = thumbnail_uri_for_path(&path)?;
        Some(Self {
            pane_id,
            generation,
            item_id,
            path,
            uri,
            modified_secs,
            mime_type: mime_type
                .map(|mime| mime.trim().to_string())
                .filter(|mime| !mime.is_empty()),
            priority,
        })
    }

    pub fn pane_id(&self) -> PaneId {
        self.pane_id
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn item_id(&self) -> ItemId {
        self.item_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn modified_secs(&self) -> u64 {
        self.modified_secs
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    pub fn priority(&self) -> ThumbnailRequestPriority {
        self.priority
    }

    fn key(&self) -> ThumbnailRequestKey {
        ThumbnailRequestKey {
            pane_id: self.pane_id,
            generation: self.generation,
            item_id: self.item_id,
            uri: self.uri.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ThumbnailRequestQueue {
    visible: VecDeque<ThumbnailRequest>,
    deferred: VecDeque<ThumbnailRequest>,
    pending: HashMap<ThumbnailRequestKey, ThumbnailRequestPriority>,
}

impl ThumbnailRequestQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn enqueue(&mut self, request: ThumbnailRequest) -> bool {
        let key = request.key();
        match self.pending.get(&key).copied() {
            Some(ThumbnailRequestPriority::Visible) => false,
            Some(ThumbnailRequestPriority::Deferred) => {
                if request.priority != ThumbnailRequestPriority::Visible {
                    return false;
                }
                self.deferred.retain(|existing| existing.key() != key);
                self.visible.push_back(request);
                self.pending.insert(key, ThumbnailRequestPriority::Visible);
                true
            }
            None => {
                let priority = request.priority;
                match priority {
                    ThumbnailRequestPriority::Visible => self.visible.push_back(request),
                    ThumbnailRequestPriority::Deferred => self.deferred.push_back(request),
                }
                self.pending.insert(key, priority);
                true
            }
        }
    }

    pub fn contains(&self, request: &ThumbnailRequest) -> bool {
        self.pending.contains_key(&request.key())
    }

    pub fn enqueue_path(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        priority: ThumbnailRequestPriority,
    ) -> bool {
        ThumbnailRequest::new(pane_id, generation, item_id, path, priority)
            .is_some_and(|request| self.enqueue(request))
    }

    pub fn enqueue_entry_metadata(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        modified_secs: u64,
        priority: ThumbnailRequestPriority,
    ) -> bool {
        ThumbnailRequest::from_entry_metadata(
            pane_id,
            generation,
            item_id,
            path,
            modified_secs,
            priority,
        )
        .is_some_and(|request| self.enqueue(request))
    }

    pub fn pop_next(&mut self) -> Option<ThumbnailRequest> {
        let request = self
            .visible
            .pop_front()
            .or_else(|| self.deferred.pop_front())?;
        self.pending.remove(&request.key());
        Some(request)
    }

    pub fn cancel_stale_generations(
        &mut self,
        pane_id: PaneId,
        current_generation: Generation,
    ) -> usize {
        self.remove_matching(|request| {
            request.pane_id == pane_id && request.generation != current_generation
        })
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) -> usize {
        self.remove_matching(|request| request.pane_id == pane_id)
    }

    pub fn cancel_deferred_matching(
        &mut self,
        predicate: impl Fn(&ThumbnailRequest) -> bool,
    ) -> Vec<ThumbnailRequest> {
        let mut removed = Vec::new();
        self.deferred.retain(|request| {
            if predicate(request) {
                removed.push(request.clone());
                false
            } else {
                true
            }
        });
        for request in &removed {
            self.pending.remove(&request.key());
        }
        removed
    }

    pub fn cancel_matching(
        &mut self,
        predicate: impl Fn(&ThumbnailRequest) -> bool,
    ) -> Vec<ThumbnailRequest> {
        let mut removed = Vec::new();
        self.visible.retain(|request| {
            if predicate(request) {
                removed.push(request.clone());
                false
            } else {
                true
            }
        });
        self.deferred.retain(|request| {
            if predicate(request) {
                removed.push(request.clone());
                false
            } else {
                true
            }
        });
        for request in &removed {
            self.pending.remove(&request.key());
        }
        removed
    }

    fn remove_matching(&mut self, predicate: impl Fn(&ThumbnailRequest) -> bool) -> usize {
        let mut removed = remove_matching_from_queue(&mut self.visible, &predicate);
        removed.extend(remove_matching_from_queue(&mut self.deferred, &predicate));
        for key in &removed {
            self.pending.remove(key);
        }
        removed.len()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThumbnailerRegistry {
    thumbnailers: Vec<ThumbnailerDefinition>,
}

impl ThumbnailerRegistry {
    pub fn shared_system() -> &'static Self {
        static REGISTRY: OnceLock<ThumbnailerRegistry> = OnceLock::new();
        REGISTRY.get_or_init(Self::load_system)
    }

    pub fn load_system() -> Self {
        Self::load_from_dirs(thumbnailer_search_dirs())
    }

    pub fn load_from_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut thumbnailers = Vec::new();
        for dir in dirs {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(OsStr::to_str) != Some("thumbnailer") {
                    continue;
                }
                let Ok(contents) = fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(thumbnailer) = parse_thumbnailer_definition(&contents)
                    && thumbnailer.try_exec_is_available()
                {
                    thumbnailers.push(thumbnailer);
                }
            }
        }
        Self { thumbnailers }
    }

    pub fn commands_for_request(
        &self,
        request: &ThumbnailRequest,
        output: &Path,
        size: ThumbnailSize,
    ) -> Vec<ExternalThumbnailerCommand> {
        let mut commands = Vec::new();
        if let Some(mime_type) = request.mime_type() {
            commands.extend(
                self.thumbnailers
                    .iter()
                    .filter(|thumbnailer| thumbnailer.matches_mime(mime_type))
                    .filter_map(|thumbnailer| {
                        thumbnailer.command_for(request.path(), request.uri(), output, size)
                    }),
            );
        }
        if commands.is_empty() {
            commands.extend(external_thumbnailer_commands_for_path(
                request.path(),
                output,
                size,
            ));
        }
        commands
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThumbnailerDefinition {
    exec: String,
    try_exec: Option<String>,
    mime_types: Vec<String>,
}

impl ThumbnailerDefinition {
    fn matches_mime(&self, mime_type: &str) -> bool {
        self.mime_types
            .iter()
            .any(|mime| thumbnailer_mime_matches(mime, mime_type))
    }

    fn command_for(
        &self,
        input: &Path,
        uri: &str,
        output: &Path,
        size: ThumbnailSize,
    ) -> Option<ExternalThumbnailerCommand> {
        expand_thumbnailer_exec(&self.exec, input, uri, output, size)
    }

    fn try_exec_is_available(&self) -> bool {
        self.try_exec.as_deref().is_none_or(program_exists_in_path)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ThumbnailRequestKey {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    uri: String,
}

pub fn default_thumbnail_cache_root() -> PathBuf {
    default_cache_home().join(THUMBNAILS_DIR)
}

pub fn thumbnail_cache_root(cache_home: &Path) -> PathBuf {
    cache_home.join(THUMBNAILS_DIR)
}

pub fn thumbnail_uri_for_path(path: &Path) -> Option<String> {
    path.is_absolute().then(|| {
        let mut uri = String::from("file://");
        uri.push_str(&percent_encode_path(path));
        uri
    })
}

pub fn thumbnail_cache_key(uri: &str) -> String {
    md5_hex(uri.as_bytes())
}

pub fn thumbnail_cache_path(root: &Path, size: ThumbnailSize, uri: &str) -> PathBuf {
    root.join(size.cache_dir())
        .join(format!("{}.{}", thumbnail_cache_key(uri), PNG_EXTENSION))
}

pub fn thumbnail_failure_path(root: &Path, uri: &str) -> PathBuf {
    root.join(FAIL_DIR).join(FAIL_APP_DIR).join(format!(
        "{}.{}",
        thumbnail_cache_key(uri),
        PNG_EXTENSION
    ))
}

pub fn thumbnail_cache_paths_for_uri(root: &Path, uri: &str) -> ThumbnailCachePaths {
    ThumbnailCachePaths {
        normal: thumbnail_cache_path(root, ThumbnailSize::Normal, uri),
        large: thumbnail_cache_path(root, ThumbnailSize::Large, uri),
        failure: thumbnail_failure_path(root, uri),
    }
}

pub fn cached_thumbnail_for_uri(root: &Path, uri: &str) -> Option<ThumbnailCacheHit> {
    cached_thumbnail(root, uri, None)
}

pub fn cached_thumbnail_for_path(root: &Path, path: &Path) -> Option<ThumbnailCacheHit> {
    let uri = thumbnail_uri_for_path(path)?;
    let modified_secs = file_modified_secs(path)?;
    cached_thumbnail(root, &uri, Some(modified_secs))
}

pub fn cached_thumbnail_for_request(
    root: &Path,
    request: &ThumbnailRequest,
) -> Option<ThumbnailCacheHit> {
    cached_thumbnail(root, request.uri(), Some(request.modified_secs()))
}

pub fn thumbnail_metadata(path: &Path) -> io::Result<ThumbnailMetadata> {
    thumbnail_metadata_from_bytes(&fs::read(path)?)
}

fn cached_thumbnail(
    root: &Path,
    uri: &str,
    modified_secs: Option<u64>,
) -> Option<ThumbnailCacheHit> {
    [ThumbnailSize::Normal, ThumbnailSize::Large]
        .into_iter()
        .find_map(|size| {
            let path = thumbnail_cache_path(root, size, uri);
            thumbnail_metadata_matches(&path, uri, modified_secs)
                .then_some(ThumbnailCacheHit { size, path })
        })
}

fn thumbnail_metadata_matches(path: &Path, uri: &str, modified_secs: Option<u64>) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(metadata) = thumbnail_metadata(path) else {
        return false;
    };
    if metadata.uri.as_deref() != Some(uri) {
        return false;
    }
    modified_secs.is_none_or(|expected| metadata.mtime == Some(expected))
}

fn file_modified_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_secs())
        })
}

fn remove_matching_from_queue(
    queue: &mut VecDeque<ThumbnailRequest>,
    predicate: &impl Fn(&ThumbnailRequest) -> bool,
) -> Vec<ThumbnailRequestKey> {
    let mut removed = Vec::new();
    queue.retain(|request| {
        if predicate(request) {
            removed.push(request.key());
            false
        } else {
            true
        }
    });
    removed
}

pub fn thumbnail_failure_is_cached(root: &Path, uri: &str, modified_secs: u64) -> bool {
    thumbnail_metadata_matches(&thumbnail_failure_path(root, uri), uri, Some(modified_secs))
}

pub fn record_thumbnail_failure(root: &Path, uri: &str, modified_secs: u64) -> io::Result<PathBuf> {
    let path = thumbnail_failure_path(root, uri);
    if !thumbnail_failure_is_cached(root, uri, modified_secs) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, failure_thumbnail_png(uri, modified_secs))?;
    }
    Ok(path)
}

pub fn generate_thumbnail_with_external_thumbnailer(
    root: &Path,
    request: &ThumbnailRequest,
) -> io::Result<Option<ThumbnailCacheHit>> {
    generate_thumbnail_with_external_thumbnailer_registry(
        root,
        request,
        ThumbnailerRegistry::shared_system(),
    )
}

pub fn generate_thumbnail_with_external_thumbnailer_registry(
    root: &Path,
    request: &ThumbnailRequest,
    registry: &ThumbnailerRegistry,
) -> io::Result<Option<ThumbnailCacheHit>> {
    if let Some(hit) = cached_thumbnail_for_request(root, request) {
        return Ok(Some(hit));
    }
    if thumbnail_failure_is_cached(root, request.uri(), request.modified_secs()) {
        return Ok(None);
    }

    let output_path = thumbnail_cache_path(root, ThumbnailSize::Normal, request.uri());
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = temporary_thumbnail_path(&output_path);
    let mut attempted = false;
    if thumbnail_request_is_windows_executable(request) {
        attempted = true;
        let _ = fs::remove_file(&temp_path);
        if let Some(png) =
            windows_executable_icon_png(request.path(), ThumbnailSize::Normal.max_dimension())?
        {
            fs::write(&temp_path, png)?;
            if write_thumbnail_metadata(&temp_path, request.uri(), request.modified_secs()).is_ok()
                && fs::rename(&temp_path, &output_path).is_ok()
                && let Some(hit) = cached_thumbnail_for_request(root, request)
            {
                return Ok(Some(hit));
            }
        }
    }
    let commands = registry.commands_for_request(request, &temp_path, ThumbnailSize::Normal);
    for command in commands {
        let _ = fs::remove_file(&temp_path);
        match run_external_thumbnailer_command(&command) {
            Ok(status) => {
                attempted = true;
                if !status.success() || !temp_path.is_file() {
                    continue;
                }
                if write_thumbnail_metadata(&temp_path, request.uri(), request.modified_secs())
                    .is_ok()
                    && fs::rename(&temp_path, &output_path).is_ok()
                    && let Some(hit) = cached_thumbnail_for_request(root, request)
                {
                    return Ok(Some(hit));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                attempted = true;
            }
        }
    }
    let _ = fs::remove_file(&temp_path);

    if attempted {
        record_thumbnail_failure(root, request.uri(), request.modified_secs())?;
    }
    Ok(None)
}

fn run_external_thumbnailer_command(
    command: &ExternalThumbnailerCommand,
) -> io::Result<ExitStatus> {
    Command::new(command.program())
        .args(command.args())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
}

pub fn external_thumbnailer_commands_for_path(
    input: &Path,
    output: &Path,
    size: ThumbnailSize,
) -> Vec<ExternalThumbnailerCommand> {
    let Some(extension) = input.extension().and_then(OsStr::to_str) else {
        return Vec::new();
    };
    let extension = extension.to_ascii_lowercase();
    let input = input.as_os_str().to_os_string();
    let output = output.as_os_str().to_os_string();
    let size_arg = size.max_dimension().to_string();

    if image_thumbnail_extension(&extension) {
        return vec![ExternalThumbnailerCommand {
            program: String::from("gdk-pixbuf-thumbnailer"),
            args: vec![
                OsString::from("-s"),
                OsString::from(size_arg),
                input,
                output,
            ],
        }];
    }

    if video_thumbnail_extension(&extension) {
        return vec![
            ExternalThumbnailerCommand {
                program: String::from("ffmpegthumbnailer"),
                args: vec![
                    OsString::from("-i"),
                    input.clone(),
                    OsString::from("-o"),
                    output.clone(),
                    OsString::from("-s"),
                    OsString::from(size_arg.clone()),
                ],
            },
            ExternalThumbnailerCommand {
                program: String::from("totem-video-thumbnailer"),
                args: vec![
                    OsString::from("-s"),
                    OsString::from(size_arg),
                    input,
                    output,
                ],
            },
        ];
    }

    if document_thumbnail_extension(&extension) {
        return vec![ExternalThumbnailerCommand {
            program: String::from("evince-thumbnailer"),
            args: vec![
                OsString::from("-s"),
                OsString::from(size_arg),
                input,
                output,
            ],
        }];
    }

    Vec::new()
}

pub fn thumbnail_request_may_have_preview(path: &Path, mime_type: Option<&str>) -> bool {
    mime_type.is_some_and(thumbnail_mime_may_have_preview)
        || thumbnail_extension_may_have_preview(path)
}

fn thumbnail_request_is_windows_executable(request: &ThumbnailRequest) -> bool {
    request
        .mime_type()
        .is_some_and(windows_executable_mime_may_have_preview)
        || path_has_windows_executable_thumbnail_extension(request.path())
}

fn thumbnail_mime_may_have_preview(mime_type: &str) -> bool {
    let mime_type = mime_type.trim().to_ascii_lowercase();
    if mime_type.starts_with("text/") {
        return false;
    }
    if mime_type.starts_with("image/") {
        return true;
    }
    if matches!(
        mime_type.as_str(),
        "application/pdf"
            | "application/postscript"
            | "application/eps"
            | "application/epub+zip"
            | "application/x-mobipocket-ebook"
    ) {
        return true;
    }
    windows_executable_mime_may_have_preview(&mime_type)
}

fn windows_executable_mime_may_have_preview(mime_type: &str) -> bool {
    matches!(
        mime_type.trim().to_ascii_lowercase().as_str(),
        "application/vnd.microsoft.portable-executable"
            | "application/x-msdownload"
            | "application/x-ms-dos-executable"
    )
}

fn thumbnail_extension_may_have_preview(path: &Path) -> bool {
    if path_has_windows_executable_thumbnail_extension(path) {
        return true;
    }
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| {
            image_thumbnail_extension(&extension) || document_thumbnail_extension(&extension)
        })
}

fn path_has_windows_executable_thumbnail_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| matches!(extension.as_str(), "exe" | "scr" | "cpl" | "dll"))
}

fn thumbnailer_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        dirs.push(PathBuf::from(data_home).join("thumbnailers"));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/thumbnailers"));
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| OsString::from("/usr/local/share:/usr/share"));
    dirs.extend(env::split_paths(&data_dirs).map(|data_dir| data_dir.join("thumbnailers")));
    dirs
}

fn parse_thumbnailer_definition(contents: &str) -> Option<ThumbnailerDefinition> {
    let entry = parse_desktop_entry_group(contents, "Thumbnailer Entry")?;
    let exec = entry.get("Exec")?.trim().to_string();
    if exec.is_empty() {
        return None;
    }
    let mime_types = entry
        .get("MimeType")?
        .split(';')
        .map(str::trim)
        .filter(|mime| !mime.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if mime_types.is_empty() {
        return None;
    }
    Some(ThumbnailerDefinition {
        exec,
        try_exec: entry
            .get("TryExec")
            .map(|try_exec| try_exec.trim().to_string())
            .filter(|try_exec| !try_exec.is_empty()),
        mime_types,
    })
}

fn parse_desktop_entry_group(contents: &str, group: &str) -> Option<HashMap<String, String>> {
    let mut current_group = None::<String>;
    let mut values = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current_group = Some(section.trim().to_string());
            continue;
        }
        if current_group.as_deref() != Some(group) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        values.insert(key.to_string(), value.trim().to_string());
    }
    (!values.is_empty()).then_some(values)
}

fn thumbnailer_mime_matches(pattern: &str, mime_type: &str) -> bool {
    if pattern == mime_type {
        return true;
    }
    let Some(prefix) = pattern.strip_suffix("/*") else {
        return false;
    };
    mime_type
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

fn expand_thumbnailer_exec(
    exec: &str,
    input: &Path,
    uri: &str,
    output: &Path,
    size: ThumbnailSize,
) -> Option<ExternalThumbnailerCommand> {
    let tokens = split_exec_template(exec);
    let (program, args) = tokens.split_first()?;
    let program = expand_thumbnailer_exec_token(program, input, uri, output, size)
        .into_string()
        .ok()?;
    if program.is_empty() {
        return None;
    }
    let args = args
        .iter()
        .map(|token| expand_thumbnailer_exec_token(token, input, uri, output, size))
        .collect::<Vec<_>>();
    Some(ExternalThumbnailerCommand { program, args })
}

fn split_exec_template(exec: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in exec.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            ch if ch.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn expand_thumbnailer_exec_token(
    token: &str,
    input: &Path,
    uri: &str,
    output: &Path,
    size: ThumbnailSize,
) -> OsString {
    let mut expanded = String::new();
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            expanded.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => expanded.push('%'),
            Some('i' | 'f' | 'F') => expanded.push_str(&input.to_string_lossy()),
            Some('u' | 'U') => expanded.push_str(uri),
            Some('o') => expanded.push_str(&output.to_string_lossy()),
            Some('s') => expanded.push_str(&size.max_dimension().to_string()),
            Some('d' | 'D') => {
                if let Some(parent) = input.parent() {
                    expanded.push_str(&parent.to_string_lossy());
                }
            }
            Some('n' | 'N') => {
                if let Some(name) = input.file_name() {
                    expanded.push_str(&name.to_string_lossy());
                }
            }
            Some(other) => {
                expanded.push('%');
                expanded.push(other);
            }
            None => expanded.push('%'),
        }
    }
    OsString::from(expanded)
}

fn program_exists_in_path(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

pub fn write_thumbnail_metadata(path: &Path, uri: &str, modified_secs: u64) -> io::Result<()> {
    let bytes = fs::read(path)?;
    let bytes = thumbnail_png_with_appended_metadata(&bytes, uri, modified_secs)?;
    fs::write(path, bytes)
}

fn failure_thumbnail_png(uri: &str, modified_secs: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(PNG_SIGNATURE);
    bytes.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]));
    bytes.extend(png_text_chunk("Thumb::URI", uri));
    bytes.extend(png_text_chunk("Thumb::MTime", &modified_secs.to_string()));
    bytes.extend(png_chunk(b"IDAT", FAILURE_THUMBNAIL_IDAT));
    bytes.extend(png_chunk(b"IEND", &[]));
    bytes
}

fn thumbnail_png_with_appended_metadata(
    bytes: &[u8],
    uri: &str,
    modified_secs: u64,
) -> io::Result<Vec<u8>> {
    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail is not a PNG file",
        ));
    }

    let mut offset = PNG_SIGNATURE.len();
    while bytes.len().saturating_sub(offset) >= PNG_CHUNK_HEADER_LEN {
        let chunk_start = offset;
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        offset += PNG_CHUNK_HEADER_LEN;
        let Some(data_end) = offset.checked_add(length) else {
            return Err(invalid_png_thumbnail());
        };
        let Some(next_offset) = data_end.checked_add(PNG_CHUNK_CRC_LEN) else {
            return Err(invalid_png_thumbnail());
        };
        if next_offset > bytes.len() {
            return Err(invalid_png_thumbnail());
        }
        if chunk_type == b"IEND" {
            let mut output = Vec::with_capacity(bytes.len() + uri.len() + 80);
            output.extend(&bytes[..chunk_start]);
            output.extend(png_text_chunk("Thumb::URI", uri));
            output.extend(png_text_chunk("Thumb::MTime", &modified_secs.to_string()));
            output.extend(&bytes[chunk_start..]);
            return Ok(output);
        }
        offset = next_offset;
    }

    Err(invalid_png_thumbnail())
}

fn temporary_thumbnail_path(output_path: &Path) -> PathBuf {
    let mut file_name = output_path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("thumbnail.png"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    file_name.push(format!(".fika-{}-{nonce}.tmp", process::id()));
    output_path.with_file_name(file_name)
}

fn image_thumbnail_extension(extension: &str) -> bool {
    matches!(
        extension,
        "png"
            | "apng"
            | "jpg"
            | "jpeg"
            | "jxl"
            | "gif"
            | "bmp"
            | "tif"
            | "tiff"
            | "webp"
            | "svg"
            | "svgz"
            | "heic"
            | "heif"
            | "avif"
            | "avifs"
    )
}

fn video_thumbnail_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mp4" | "m4v" | "mkv" | "webm" | "mov" | "avi" | "flv" | "ogv" | "mpeg" | "mpg" | "wmv"
    )
}

fn document_thumbnail_extension(extension: &str) -> bool {
    matches!(extension, "pdf" | "ps" | "eps" | "epub")
}

fn png_text_chunk(key: &str, value: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(key.as_bytes());
    data.push(0);
    data.extend(value.as_bytes());
    png_chunk(b"tEXt", &data)
}

fn png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend((data.len() as u32).to_be_bytes());
    chunk.extend(chunk_type);
    chunk.extend(data);
    chunk.extend(png_crc32(chunk_type, data).to_be_bytes());
    chunk
}

fn png_crc32(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in chunk_type.iter().chain(data.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn default_cache_home() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"))
}

fn percent_encode_path(path: &Path) -> String {
    let bytes = path_bytes(path);
    let mut encoded = String::new();
    for byte in bytes {
        if uri_path_byte_is_unreserved(byte) || byte == b'/' {
            encoded.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

fn uri_path_byte_is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn thumbnail_metadata_from_bytes(bytes: &[u8]) -> io::Result<ThumbnailMetadata> {
    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail is not a PNG file",
        ));
    }

    let mut metadata = ThumbnailMetadata::default();
    let mut offset = PNG_SIGNATURE.len();
    while bytes.len().saturating_sub(offset) >= PNG_CHUNK_HEADER_LEN {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        offset += PNG_CHUNK_HEADER_LEN;
        let Some(data_end) = offset.checked_add(length) else {
            return Err(invalid_png_thumbnail());
        };
        let Some(next_offset) = data_end.checked_add(PNG_CHUNK_CRC_LEN) else {
            return Err(invalid_png_thumbnail());
        };
        if next_offset > bytes.len() {
            return Err(invalid_png_thumbnail());
        }

        let data = &bytes[offset..data_end];
        if chunk_type == b"tEXt" {
            read_thumbnail_text_chunk(data, &mut metadata);
        }

        offset = next_offset;
        if chunk_type == b"IEND" {
            break;
        }
    }
    Ok(metadata)
}

fn read_thumbnail_text_chunk(data: &[u8], metadata: &mut ThumbnailMetadata) {
    let Some(separator) = data.iter().position(|byte| *byte == 0) else {
        return;
    };
    let key = &data[..separator];
    let value = &data[separator + 1..];
    match key {
        b"Thumb::URI" => {
            if let Ok(uri) = std::str::from_utf8(value) {
                metadata.uri = Some(uri.to_string());
            }
        }
        b"Thumb::MTime" => {
            if let Ok(value) = std::str::from_utf8(value)
                && let Ok(mtime) = value.parse::<u64>()
            {
                metadata.mtime = Some(mtime);
            }
        }
        _ => {}
    }
}

fn invalid_png_thumbnail() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "thumbnail PNG has truncated chunk data",
    )
}

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut message = input.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend(bit_len.to_le_bytes());

    let mut a0 = 0x67452301u32;
    let mut b0 = 0xefcdab89u32;
    let mut c0 = 0x98badcfeu32;
    let mut d0 = 0x10325476u32;

    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_le_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }

        let mut a = a0;
        let mut b = b0;
        let mut c = c0;
        let mut d = d0;

        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let next = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(words[g])
                    .rotate_left(S[i]),
            );
            a = next;
        }

        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());
    bytes_to_hex(&digest)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn thumbnail_uri_percent_encodes_file_path() {
        let uri = thumbnail_uri_for_path(Path::new("/tmp/Fika Test/value#1.txt")).unwrap();
        assert_eq!(uri, "file:///tmp/Fika%20Test/value%231.txt");
        assert!(thumbnail_uri_for_path(Path::new("relative.txt")).is_none());
    }

    #[test]
    fn freedesktop_thumbnail_hash_uses_md5_uri() {
        assert_eq!(thumbnail_cache_key(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            thumbnail_cache_key("abc"),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            thumbnail_cache_key("file:///tmp/Fika%20Test/value%231.txt"),
            "4869a68b8abd00bef4bb8d34392b25c7"
        );
    }

    #[test]
    fn thumbnail_cache_lookup_prefers_normal_before_large() {
        let root = temp_root("lookup");
        let uri = "file:///tmp/image.png";
        let large = thumbnail_cache_path(&root, ThumbnailSize::Large, uri);
        let normal = thumbnail_cache_path(&root, ThumbnailSize::Normal, uri);
        fs::create_dir_all(large.parent().unwrap()).unwrap();
        fs::write(&large, test_thumbnail_png(uri, 123)).unwrap();

        let large_hit = cached_thumbnail_for_uri(&root, uri).unwrap();
        assert_eq!(large_hit.size(), ThumbnailSize::Large);
        assert_eq!(large_hit.path(), large.as_path());

        fs::create_dir_all(normal.parent().unwrap()).unwrap();
        fs::write(&normal, test_thumbnail_png(uri, 123)).unwrap();
        let normal_hit = cached_thumbnail_for_uri(&root, uri).unwrap();
        assert_eq!(normal_hit.size(), ThumbnailSize::Normal);
        assert_eq!(normal_hit.path(), normal.as_path());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_cache_lookup_rejects_mismatched_metadata() {
        let root = temp_root("mismatch");
        let file = root.join("image.png");
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, b"source").unwrap();
        let uri = thumbnail_uri_for_path(&file).unwrap();
        let mtime = fs::metadata(&file)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let thumbnail = thumbnail_cache_path(&root, ThumbnailSize::Normal, &uri);
        fs::create_dir_all(thumbnail.parent().unwrap()).unwrap();
        fs::write(
            &thumbnail,
            test_thumbnail_png("file:///tmp/other.png", mtime),
        )
        .unwrap();
        assert!(cached_thumbnail_for_path(&root, &file).is_none());

        fs::write(&thumbnail, test_thumbnail_png(&uri, mtime + 1)).unwrap();
        assert!(cached_thumbnail_for_path(&root, &file).is_none());

        fs::write(&thumbnail, test_thumbnail_png(&uri, mtime)).unwrap();
        assert_eq!(
            cached_thumbnail_for_path(&root, &file).unwrap().path(),
            thumbnail.as_path()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_metadata_reads_freedesktop_text_chunks() {
        let uri = "file:///tmp/image.png";
        let metadata = thumbnail_metadata_from_bytes(&test_thumbnail_png(uri, 42)).unwrap();

        assert_eq!(metadata.uri.as_deref(), Some(uri));
        assert_eq!(metadata.mtime, Some(42));
    }

    #[test]
    fn cached_thumbnail_for_request_uses_request_mtime_without_restat() {
        let root = temp_root("request-cache");
        let path = PathBuf::from("/tmp/fika-thumbnail-request-missing.png");
        let request = ThumbnailRequest::from_entry_metadata(
            PaneId(1),
            Generation(1),
            ItemId(1),
            path,
            42,
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();
        let thumbnail = thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri());
        fs::create_dir_all(thumbnail.parent().unwrap()).unwrap();
        fs::write(&thumbnail, test_thumbnail_png(request.uri(), 42)).unwrap();

        assert_eq!(
            cached_thumbnail_for_request(&root, &request)
                .unwrap()
                .path(),
            thumbnail.as_path()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_thumbnailer_commands_match_file_kind() {
        let image = external_thumbnailer_commands_for_path(
            Path::new("/tmp/photo.JPG"),
            Path::new("/tmp/out.png"),
            ThumbnailSize::Normal,
        );
        assert_eq!(image.len(), 1);
        assert_eq!(image[0].program(), "gdk-pixbuf-thumbnailer");
        assert_eq!(image[0].args()[0], OsString::from("-s"));
        assert_eq!(image[0].args()[1], OsString::from("128"));

        let video = external_thumbnailer_commands_for_path(
            Path::new("/tmp/movie.webm"),
            Path::new("/tmp/out.png"),
            ThumbnailSize::Large,
        );
        assert_eq!(video.len(), 2);
        assert_eq!(video[0].program(), "ffmpegthumbnailer");
        assert_eq!(video[1].program(), "totem-video-thumbnailer");
        assert!(video[0].args().contains(&OsString::from("256")));

        let document = external_thumbnailer_commands_for_path(
            Path::new("/tmp/document.pdf"),
            Path::new("/tmp/out.png"),
            ThumbnailSize::Normal,
        );
        assert_eq!(document.len(), 1);
        assert_eq!(document[0].program(), "evince-thumbnailer");

        assert!(
            external_thumbnailer_commands_for_path(
                Path::new("/tmp/archive.zip"),
                Path::new("/tmp/out.png"),
                ThumbnailSize::Normal,
            )
            .is_empty()
        );
    }

    #[test]
    fn thumbnail_preview_filter_matches_dolphin_preview_candidates() {
        assert!(!thumbnail_request_may_have_preview(
            Path::new("/tmp/notes.txt"),
            Some("text/plain")
        ));
        assert!(thumbnail_request_may_have_preview(
            Path::new("/tmp/photo"),
            Some("image/png")
        ));
        assert!(thumbnail_request_may_have_preview(
            Path::new("/tmp/photo.png"),
            Some("application/octet-stream")
        ));
        assert!(thumbnail_request_may_have_preview(
            Path::new("/tmp/clip.avifs"),
            Some("application/octet-stream")
        ));
        assert!(thumbnail_request_may_have_preview(
            Path::new("/tmp/setup.exe"),
            Some("application/vnd.microsoft.portable-executable")
        ));
        assert!(thumbnail_request_may_have_preview(
            Path::new("/tmp/setup.exe"),
            Some("application/octet-stream")
        ));
        assert!(!thumbnail_request_may_have_preview(
            Path::new("/tmp/clip.mp4"),
            Some("video/mp4")
        ));
        assert!(!thumbnail_request_may_have_preview(
            Path::new("/tmp/clip.mp4"),
            Some("application/octet-stream")
        ));
        assert!(!thumbnail_request_may_have_preview(
            Path::new("/tmp/archive.zip"),
            Some("application/zip")
        ));
    }

    #[test]
    fn thumbnailer_registry_parses_and_expands_desktop_exec() {
        let root = temp_root("thumbnailer-registry");
        let dir = root.join("thumbnailers");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("custom.thumbnailer"),
            "[Thumbnailer Entry]\n\
             Exec=custom-thumbnailer --size %s --input %i --uri %u --output %o\n\
             MimeType=image/png;image/jpeg;\n",
        )
        .unwrap();
        let registry = ThumbnailerRegistry::load_from_dirs([dir]);
        let request = ThumbnailRequest::from_entry_metadata_with_mime(
            PaneId(1),
            Generation(1),
            ItemId(1),
            PathBuf::from("/tmp/photo.png"),
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();

        let commands = registry.commands_for_request(
            &request,
            Path::new("/tmp/out.png"),
            ThumbnailSize::Large,
        );

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program(), "custom-thumbnailer");
        assert_eq!(
            commands[0].args(),
            &[
                OsString::from("--size"),
                OsString::from("256"),
                OsString::from("--input"),
                OsString::from("/tmp/photo.png"),
                OsString::from("--uri"),
                OsString::from("file:///tmp/photo.png"),
                OsString::from("--output"),
                OsString::from("/tmp/out.png"),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnailer_exec_expands_common_freedesktop_file_field_codes() {
        let root = temp_root("thumbnailer-field-codes");
        let dir = root.join("thumbnailers");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("field-codes.thumbnailer"),
            "[Thumbnailer Entry]\n\
             Exec=field-thumb --file %f --files %F --url %U --dir %d --dirs %D --name %n --names %N --literal %% \"--quoted=%f\"\n\
             MimeType=image/png;\n",
        )
        .unwrap();
        let registry = ThumbnailerRegistry::load_from_dirs([dir]);
        let request = ThumbnailRequest::from_entry_metadata_with_mime(
            PaneId(1),
            Generation(1),
            ItemId(1),
            PathBuf::from("/tmp/Fika Test/photo one.png"),
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();

        let commands = registry.commands_for_request(
            &request,
            Path::new("/tmp/out image.png"),
            ThumbnailSize::Normal,
        );

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program(), "field-thumb");
        assert_eq!(
            commands[0].args(),
            &[
                OsString::from("--file"),
                OsString::from("/tmp/Fika Test/photo one.png"),
                OsString::from("--files"),
                OsString::from("/tmp/Fika Test/photo one.png"),
                OsString::from("--url"),
                OsString::from("file:///tmp/Fika%20Test/photo%20one.png"),
                OsString::from("--dir"),
                OsString::from("/tmp/Fika Test"),
                OsString::from("--dirs"),
                OsString::from("/tmp/Fika Test"),
                OsString::from("--name"),
                OsString::from("photo one.png"),
                OsString::from("--names"),
                OsString::from("photo one.png"),
                OsString::from("--literal"),
                OsString::from("%"),
                OsString::from("--quoted=/tmp/Fika Test/photo one.png"),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnailer_registry_matches_wildcard_mime_before_extension_fallback() {
        let root = temp_root("thumbnailer-wildcard");
        let dir = root.join("thumbnailers");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("wildcard.thumbnailer"),
            "[Thumbnailer Entry]\nExec=wild-thumb %i %o\nMimeType=image/*;\n",
        )
        .unwrap();
        let registry = ThumbnailerRegistry::load_from_dirs([dir]);
        let request = ThumbnailRequest::from_entry_metadata_with_mime(
            PaneId(1),
            Generation(1),
            ItemId(1),
            PathBuf::from("/tmp/photo.png"),
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();

        let commands = registry.commands_for_request(
            &request,
            Path::new("/tmp/out.png"),
            ThumbnailSize::Normal,
        );

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program(), "wild-thumb");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_thumbnail_metadata_appends_freedesktop_text_chunks() {
        let root = temp_root("write-metadata");
        fs::create_dir_all(&root).unwrap();
        let thumbnail = root.join("thumb.png");
        fs::write(&thumbnail, test_thumbnail_png_without_metadata()).unwrap();

        write_thumbnail_metadata(&thumbnail, "file:///tmp/image.png", 42).unwrap();

        let metadata = thumbnail_metadata(&thumbnail).unwrap();
        assert_eq!(metadata.uri.as_deref(), Some("file:///tmp/image.png"));
        assert_eq!(metadata.mtime, Some(42));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn external_thumbnailer_generation_writes_normal_cache_with_metadata() {
        let root = temp_root("external-generate");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.png");
        let fixture = root.join("fixture.png");
        let script = root.join("thumbnailer.sh");
        let thumbnailer_dir = root.join("thumbnailers");
        fs::write(&source, b"source").unwrap();
        fs::write(&fixture, test_thumbnail_png_without_metadata()).unwrap();
        write_executable_script(
            &script,
            format!("#!/bin/sh\n/bin/cp {} \"$2\"\n", sh_quote_path(&fixture)),
        );
        fs::create_dir_all(&thumbnailer_dir).unwrap();
        fs::write(
            thumbnailer_dir.join("fika.thumbnailer"),
            format!(
                "[Thumbnailer Entry]\nExec={} %i %o\nMimeType=image/png;\n",
                exec_quote_path(&script)
            ),
        )
        .unwrap();
        let registry = ThumbnailerRegistry::load_from_dirs([thumbnailer_dir]);
        let request = ThumbnailRequest::from_entry_metadata_with_mime(
            PaneId(1),
            Generation(1),
            ItemId(1),
            source,
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();

        let hit = generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
            .unwrap()
            .unwrap();

        let expected = thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri());
        assert_eq!(hit.size(), ThumbnailSize::Normal);
        assert_eq!(hit.path(), expected.as_path());
        let metadata = thumbnail_metadata(&expected).unwrap();
        assert_eq!(metadata.uri.as_deref(), Some(request.uri()));
        assert_eq!(metadata.mtime, Some(42));
        assert!(!thumbnail_failure_is_cached(
            &root,
            request.uri(),
            request.modified_secs()
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn external_thumbnailer_runs_with_suppressed_stdio() {
        let root = temp_root("external-stdio");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.png");
        let fixture = root.join("fixture.png");
        let script = root.join("thumbnailer.sh");
        let thumbnailer_dir = root.join("thumbnailers");
        fs::write(&source, b"source").unwrap();
        fs::write(&fixture, test_thumbnail_png_without_metadata()).unwrap();
        write_executable_script(
            &script,
            format!(
                "#!/bin/sh\n\
                 [ \"$(readlink /proc/$$/fd/1)\" = /dev/null ] || exit 11\n\
                 [ \"$(readlink /proc/$$/fd/2)\" = /dev/null ] || exit 12\n\
                 echo hidden stdout\n\
                 echo hidden stderr >&2\n\
                 /bin/cp {} \"$2\"\n",
                sh_quote_path(&fixture)
            ),
        );
        fs::create_dir_all(&thumbnailer_dir).unwrap();
        fs::write(
            thumbnailer_dir.join("fika.thumbnailer"),
            format!(
                "[Thumbnailer Entry]\nExec={} %i %o\nMimeType=image/png;\n",
                exec_quote_path(&script)
            ),
        )
        .unwrap();
        let registry = ThumbnailerRegistry::load_from_dirs([thumbnailer_dir]);
        let request = ThumbnailRequest::from_entry_metadata_with_mime(
            PaneId(1),
            Generation(1),
            ItemId(1),
            source,
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();

        let hit = generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
            .unwrap()
            .unwrap();

        assert_eq!(
            hit.path(),
            thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri()).as_path()
        );
        assert!(!thumbnail_failure_is_cached(
            &root,
            request.uri(),
            request.modified_secs()
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn external_thumbnailer_failure_records_marker_and_skips_retry() {
        let root = temp_root("external-failure");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("broken.png");
        let script = root.join("thumbnailer.sh");
        let attempts = root.join("attempts.txt");
        let thumbnailer_dir = root.join("thumbnailers");
        fs::write(&source, b"broken").unwrap();
        write_executable_script(
            &script,
            format!(
                "#!/bin/sh\necho attempt >> {}\nexit 2\n",
                sh_quote_path(&attempts)
            ),
        );
        fs::create_dir_all(&thumbnailer_dir).unwrap();
        fs::write(
            thumbnailer_dir.join("fika.thumbnailer"),
            format!(
                "[Thumbnailer Entry]\nExec={} %i %o\nMimeType=image/png;\n",
                exec_quote_path(&script)
            ),
        )
        .unwrap();
        let registry = ThumbnailerRegistry::load_from_dirs([thumbnailer_dir]);
        let request = ThumbnailRequest::from_entry_metadata_with_mime(
            PaneId(1),
            Generation(1),
            ItemId(1),
            source,
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();

        assert!(
            generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
                .unwrap()
                .is_none()
        );
        assert_eq!(fs::read_to_string(&attempts).unwrap(), "attempt\n");
        assert!(thumbnail_failure_is_cached(
            &root,
            request.uri(),
            request.modified_secs()
        ));
        assert!(!thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri()).is_file());
        let metadata = thumbnail_metadata(&thumbnail_failure_path(&root, request.uri())).unwrap();
        assert_eq!(metadata.uri.as_deref(), Some(request.uri()));
        assert_eq!(metadata.mtime, Some(42));

        assert!(
            generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
                .unwrap()
                .is_none()
        );
        assert_eq!(fs::read_to_string(&attempts).unwrap(), "attempt\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_request_queue_schedules_visible_before_deferred() {
        let root = temp_root("queue-visible");
        fs::create_dir_all(&root).unwrap();
        let mut queue = ThumbnailRequestQueue::new();

        assert!(queue.enqueue(test_request(
            &root,
            "deferred.png",
            ItemId(1),
            Generation(1),
            ThumbnailRequestPriority::Deferred,
        )));
        assert!(queue.enqueue(test_request(
            &root,
            "visible.png",
            ItemId(2),
            Generation(1),
            ThumbnailRequestPriority::Visible,
        )));

        let first = queue.pop_next().unwrap();
        assert_eq!(first.item_id(), ItemId(2));
        assert_eq!(first.priority(), ThumbnailRequestPriority::Visible);
        assert_eq!(queue.pop_next().unwrap().item_id(), ItemId(1));
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_request_uses_entry_metadata_without_restat() {
        let path = PathBuf::from("/tmp/fika-thumbnail-metadata-missing.png");

        let request = ThumbnailRequest::from_entry_metadata(
            PaneId(1),
            Generation(2),
            ItemId(3),
            path.clone(),
            42,
            ThumbnailRequestPriority::Visible,
        )
        .unwrap();
        assert_eq!(request.modified_secs(), 42);
        assert_eq!(
            request.uri(),
            "file:///tmp/fika-thumbnail-metadata-missing.png"
        );

        assert!(
            ThumbnailRequest::new(
                PaneId(1),
                Generation(2),
                ItemId(3),
                path,
                ThumbnailRequestPriority::Visible,
            )
            .is_none()
        );
    }

    #[test]
    fn thumbnail_request_queue_deduplicates_and_promotes_visible_requests() {
        let root = temp_root("queue-dedup");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("same.png");
        fs::write(&path, b"source").unwrap();
        let mut queue = ThumbnailRequestQueue::new();

        assert!(queue.enqueue_path(
            PaneId(1),
            Generation(1),
            ItemId(1),
            path.clone(),
            ThumbnailRequestPriority::Deferred,
        ));
        assert!(!queue.enqueue_path(
            PaneId(1),
            Generation(1),
            ItemId(1),
            path.clone(),
            ThumbnailRequestPriority::Deferred,
        ));
        assert!(queue.enqueue_path(
            PaneId(1),
            Generation(1),
            ItemId(1),
            path,
            ThumbnailRequestPriority::Visible,
        ));
        assert_eq!(queue.len(), 1);

        let request = queue.pop_next().unwrap();
        assert_eq!(request.item_id(), ItemId(1));
        assert_eq!(request.priority(), ThumbnailRequestPriority::Visible);
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_request_queue_cancels_stale_generations_for_navigation() {
        let root = temp_root("queue-cancel");
        fs::create_dir_all(&root).unwrap();
        let mut queue = ThumbnailRequestQueue::new();

        assert!(queue.enqueue(test_request(
            &root,
            "old.png",
            ItemId(1),
            Generation(1),
            ThumbnailRequestPriority::Visible,
        )));
        assert!(queue.enqueue(test_request(
            &root,
            "current.png",
            ItemId(2),
            Generation(2),
            ThumbnailRequestPriority::Deferred,
        )));
        assert!(
            queue.enqueue(
                ThumbnailRequest::new(
                    PaneId(2),
                    Generation(1),
                    ItemId(3),
                    write_source(&root, "other-pane.png"),
                    ThumbnailRequestPriority::Visible,
                )
                .unwrap(),
            )
        );

        assert_eq!(queue.cancel_stale_generations(PaneId(1), Generation(2)), 1);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop_next().unwrap().pane_id(), PaneId(2));
        let current = queue.pop_next().unwrap();
        assert_eq!(current.pane_id(), PaneId(1));
        assert_eq!(current.generation(), Generation(2));
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_request_queue_contains_tracks_pending_requests() {
        let root = temp_root("queue-contains");
        fs::create_dir_all(&root).unwrap();
        let mut queue = ThumbnailRequestQueue::new();
        let request = test_request(
            &root,
            "visible.png",
            ItemId(1),
            Generation(1),
            ThumbnailRequestPriority::Visible,
        );

        assert!(!queue.contains(&request));
        assert!(queue.enqueue(request.clone()));
        assert!(queue.contains(&request));

        let popped = queue.pop_next().unwrap();
        assert_eq!(popped.item_id(), request.item_id());
        assert!(!queue.contains(&request));
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_request_queue_cancels_only_matching_deferred_requests() {
        let root = temp_root("queue-cancel-deferred");
        fs::create_dir_all(&root).unwrap();
        let mut queue = ThumbnailRequestQueue::new();
        let visible = test_request(
            &root,
            "visible.png",
            ItemId(1),
            Generation(1),
            ThumbnailRequestPriority::Visible,
        );
        let keep_deferred = test_request(
            &root,
            "keep-deferred.png",
            ItemId(2),
            Generation(1),
            ThumbnailRequestPriority::Deferred,
        );
        let remove_deferred = test_request(
            &root,
            "remove-deferred.png",
            ItemId(3),
            Generation(1),
            ThumbnailRequestPriority::Deferred,
        );

        assert!(queue.enqueue(visible.clone()));
        assert!(queue.enqueue(keep_deferred.clone()));
        assert!(queue.enqueue(remove_deferred.clone()));

        let removed = queue.cancel_deferred_matching(|request| request.item_id() == ItemId(3));

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].item_id(), ItemId(3));
        assert!(queue.contains(&visible));
        assert!(queue.contains(&keep_deferred));
        assert!(!queue.contains(&remove_deferred));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop_next().unwrap().item_id(), ItemId(1));
        assert_eq!(queue.pop_next().unwrap().item_id(), ItemId(2));
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_failure_marker_uses_freedesktop_fail_path() {
        let root = temp_root("failure");
        let uri = "file:///tmp/broken.png";
        let path = record_thumbnail_failure(&root, uri, 123).unwrap();

        assert_eq!(path, thumbnail_failure_path(&root, uri));
        assert!(thumbnail_failure_is_cached(&root, uri, 123));
        assert!(!thumbnail_failure_is_cached(&root, uri, 124));
        let metadata = thumbnail_metadata(&path).unwrap();
        assert_eq!(metadata.uri.as_deref(), Some(uri));
        assert_eq!(metadata.mtime, Some(123));
        let bytes = fs::read(path).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_failure_marker_overwrites_stale_metadata() {
        let root = temp_root("failure-stale");
        let uri = "file:///tmp/changed.png";
        let path = record_thumbnail_failure(&root, uri, 123).unwrap();
        assert!(thumbnail_failure_is_cached(&root, uri, 123));

        assert_eq!(record_thumbnail_failure(&root, uri, 456).unwrap(), path);

        assert!(!thumbnail_failure_is_cached(&root, uri, 123));
        assert!(thumbnail_failure_is_cached(&root, uri, 456));
        let metadata = thumbnail_metadata(&path).unwrap();
        assert_eq!(metadata.uri.as_deref(), Some(uri));
        assert_eq!(metadata.mtime, Some(456));

        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!("fika-thumbnail-{name}-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn test_request(
        root: &Path,
        name: &str,
        item_id: ItemId,
        generation: Generation,
        priority: ThumbnailRequestPriority,
    ) -> ThumbnailRequest {
        ThumbnailRequest::new(
            PaneId(1),
            generation,
            item_id,
            write_source(root, name),
            priority,
        )
        .unwrap()
    }

    fn write_source(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        fs::write(&path, b"source").unwrap();
        path
    }

    #[cfg(unix)]
    fn write_executable_script(path: &Path, contents: String) {
        fs::write(path, contents).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    fn exec_quote_path(path: &Path) -> String {
        let path = path.to_string_lossy();
        format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
    }

    #[cfg(unix)]
    fn sh_quote_path(path: &Path) -> String {
        let path = path.to_string_lossy();
        format!("'{}'", path.replace('\'', "'\\''"))
    }

    pub(crate) fn test_thumbnail_png(uri: &str, mtime: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(PNG_SIGNATURE);
        bytes.extend(test_png_chunk(b"IHDR", &[0; 13]));
        bytes.extend(test_png_text_chunk("Thumb::URI", uri));
        bytes.extend(test_png_text_chunk("Thumb::MTime", &mtime.to_string()));
        bytes.extend(test_png_chunk(b"IEND", &[]));
        bytes
    }

    fn test_thumbnail_png_without_metadata() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(PNG_SIGNATURE);
        bytes.extend(test_png_chunk(b"IHDR", &[0; 13]));
        bytes.extend(test_png_chunk(b"IDAT", FAILURE_THUMBNAIL_IDAT));
        bytes.extend(test_png_chunk(b"IEND", &[]));
        bytes
    }

    fn test_png_text_chunk(key: &str, value: &str) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(key.as_bytes());
        data.push(0);
        data.extend(value.as_bytes());
        test_png_chunk(b"tEXt", &data)
    }

    fn test_png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::new();
        chunk.extend((data.len() as u32).to_be_bytes());
        chunk.extend(chunk_type);
        chunk.extend(data);
        chunk.extend([0; 4]);
        chunk
    }
}
