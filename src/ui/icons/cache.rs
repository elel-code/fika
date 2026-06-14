use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui::{Image, ImageFormat, RenderImage, SvgRenderer};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileIconSnapshot {
    pub(crate) icon_name: Arc<str>,
    pub(crate) path: Option<Arc<Path>>,
    pub(crate) render_image: Option<Arc<RenderImage>>,
    pub(crate) fallback_marker: Arc<str>,
    pub(crate) fallback_fg: u32,
    pub(crate) fallback_bg: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum FileIconKind {
    Directory,
    Mime {
        mime: Arc<str>,
        extension: Option<String>,
    },
    File {
        extension: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct NamedIconCacheKey {
    name: String,
    size_px: u16,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RoleIconCacheKey {
    icon_name: String,
    size_px: u16,
    fallback_marker: String,
    fallback_fg: u32,
    fallback_bg: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum FileIconLoadKey {
    Named(NamedIconCacheKey),
    Role(RoleIconCacheKey),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IconResourceCacheKey {
    candidates: Vec<String>,
    size_px: u16,
}

#[derive(Clone, Debug)]
struct FileIconLoadRequest {
    resource_key: IconResourceCacheKey,
}

#[derive(Clone, Debug)]
pub(crate) struct FileIconLoadBatch {
    resolver: Arc<Mutex<IconThemeResolver>>,
    render_images: Arc<Mutex<HashMap<PathBuf, Option<Arc<RenderImage>>>>>,
    requests: Vec<FileIconLoadRequest>,
}

#[derive(Clone, Debug)]
pub(crate) struct FileIconLoadResult {
    icons: Vec<FileIconLoadedIcon>,
}

#[derive(Clone, Debug)]
struct FileIconLoadedIcon {
    resource_key: IconResourceCacheKey,
    resource: FileIconResolvedResource,
}

#[derive(Clone, Debug)]
struct FileIconResolvedResource {
    icon_name: String,
    path: Option<PathBuf>,
    render_image: Option<Arc<RenderImage>>,
}

#[derive(Clone, Debug)]
pub(crate) struct FileIconCache {
    named_cached: HashMap<NamedIconCacheKey, FileIconSnapshot>,
    role_cached: HashMap<RoleIconCacheKey, FileIconSnapshot>,
    resource_cached: HashMap<IconResourceCacheKey, FileIconResolvedResource>,
    render_images: Arc<Mutex<HashMap<PathBuf, Option<Arc<RenderImage>>>>>,
    pending_load_keys: HashSet<IconResourceCacheKey>,
    pending_loads: VecDeque<FileIconLoadRequest>,
    resource_waiters: HashMap<IconResourceCacheKey, Vec<FileIconLoadKey>>,
    icon_load_batch_pending: bool,
    theme: Arc<Mutex<IconThemeResolver>>,
    mime: fika_core::MimeDatabase,
}

impl Default for FileIconCache {
    fn default() -> Self {
        Self {
            named_cached: HashMap::new(),
            role_cached: HashMap::new(),
            resource_cached: HashMap::new(),
            render_images: Arc::new(Mutex::new(HashMap::new())),
            pending_load_keys: HashSet::new(),
            pending_loads: VecDeque::new(),
            resource_waiters: HashMap::new(),
            icon_load_batch_pending: false,
            theme: Arc::new(Mutex::new(IconThemeResolver::default())),
            mime: fika_core::MimeDatabase::default(),
        }
    }
}

impl FileIconCache {
    pub(crate) fn icon_name_for(
        &mut self,
        path: &Path,
        is_dir: bool,
        mime_type: Option<Arc<str>>,
    ) -> Arc<str> {
        let kind = file_icon_kind(path, is_dir, mime_type);
        Arc::from(file_icon_role_name(&kind, &self.mime))
    }

    pub(crate) fn icon_for_name_role(
        &mut self,
        icon_name: &str,
        path: &Path,
        is_dir: bool,
        mime_type: Option<Arc<str>>,
        icon_size: f32,
    ) -> FileIconSnapshot {
        let key = self.ensure_role_icon_cached(icon_name, path, is_dir, mime_type, icon_size);
        self.with_cached_render_image(
            self.role_cached
                .get(&key)
                .expect("role icon cache entry missing")
                .clone(),
        )
    }

    pub(crate) fn preload_icon_for_model_role(
        &mut self,
        stored_icon_name: Option<Arc<str>>,
        path: &Path,
        is_dir: bool,
        mime_type: Option<Arc<str>>,
        icon_size: f32,
    ) {
        let icon_name =
            stored_icon_name.unwrap_or_else(|| self.icon_name_for(path, is_dir, mime_type.clone()));
        self.ensure_role_icon_cached(icon_name.as_ref(), path, is_dir, mime_type, icon_size);
    }

    fn ensure_role_icon_cached(
        &mut self,
        icon_name: &str,
        path: &Path,
        is_dir: bool,
        mime_type: Option<Arc<str>>,
        icon_size: f32,
    ) -> RoleIconCacheKey {
        let kind = file_icon_kind(path, is_dir, mime_type);
        let profile = file_icon_profile(&kind, &self.mime);
        let key = RoleIconCacheKey {
            icon_name: icon_name.to_string(),
            size_px: icon_cache_size(icon_size),
            fallback_marker: profile.marker,
            fallback_fg: profile.fg,
            fallback_bg: profile.bg,
        };
        if !self.role_cached.contains_key(&key) {
            let icon = file_icon_snapshot_from_resolved_path(
                key.icon_name.clone(),
                None,
                key.fallback_marker.clone(),
                key.fallback_fg,
                key.fallback_bg,
            );
            self.role_cached.insert(key.clone(), icon);
            self.queue_icon_resource(
                FileIconLoadKey::Role(key.clone()),
                IconResourceCacheKey {
                    candidates: vec![icon_name.to_string()],
                    size_px: icon_cache_size(icon_size),
                },
            );
        }
        key
    }

    pub(crate) fn named_icon(
        &mut self,
        name: &str,
        candidates: &[&str],
        fallback_marker: &str,
        fallback_fg: u32,
        fallback_bg: u32,
        icon_size: f32,
    ) -> FileIconSnapshot {
        let key = NamedIconCacheKey {
            name: name.to_string(),
            size_px: icon_cache_size(icon_size),
        };
        if let Some(icon) = self.named_cached.get(&key) {
            return self.with_cached_render_image(icon.clone());
        }

        let candidates = candidates
            .iter()
            .map(|candidate| (*candidate).to_string())
            .collect::<Vec<_>>();
        let icon_name = candidates
            .first()
            .cloned()
            .unwrap_or_else(|| name.to_string());
        let icon = file_icon_snapshot_from_resolved_path(
            icon_name,
            None,
            fallback_marker.to_string(),
            fallback_fg,
            fallback_bg,
        );
        self.named_cached.insert(key.clone(), icon);
        self.queue_icon_resource(
            FileIconLoadKey::Named(key.clone()),
            IconResourceCacheKey {
                candidates,
                size_px: icon_cache_size(icon_size),
            },
        );
        self.with_cached_render_image(
            self.named_cached
                .get(&key)
                .expect("named icon cache entry missing")
                .clone(),
        )
    }

    pub(crate) fn take_icon_load_batch(&mut self, limit: usize) -> Option<FileIconLoadBatch> {
        if self.icon_load_batch_pending || self.pending_loads.is_empty() {
            return None;
        }
        let limit = limit.max(1);
        let mut requests = Vec::with_capacity(limit.min(self.pending_loads.len()));
        while requests.len() < limit {
            let Some(request) = self.pending_loads.pop_front() else {
                break;
            };
            requests.push(request);
        }
        self.icon_load_batch_pending = true;
        Some(FileIconLoadBatch {
            resolver: Arc::clone(&self.theme),
            render_images: Arc::clone(&self.render_images),
            requests,
        })
    }

    pub(crate) fn load_icon_batch(batch: FileIconLoadBatch) -> FileIconLoadResult {
        let icons = batch
            .requests
            .into_iter()
            .map(|request| {
                let (icon_name, path) = resolve_icon_load_request(
                    &batch.resolver,
                    &request.resource_key.candidates,
                    request.resource_key.size_px,
                );
                let render_image = path.as_ref().and_then(|path| {
                    if let Some(image) = cached_render_image(&batch.render_images, path) {
                        return image;
                    }
                    let image = load_icon_render_image(path);
                    batch
                        .render_images
                        .lock()
                        .expect("icon render image cache poisoned")
                        .insert(path.clone(), image.clone());
                    image
                });
                FileIconLoadedIcon {
                    resource_key: request.resource_key,
                    resource: FileIconResolvedResource {
                        icon_name,
                        path,
                        render_image,
                    },
                }
            })
            .collect();
        FileIconLoadResult { icons }
    }

    pub(crate) fn finish_icon_load_batch(&mut self, result: FileIconLoadResult) -> bool {
        self.icon_load_batch_pending = false;
        let mut changed = false;
        for icon in result.icons {
            self.pending_load_keys.remove(&icon.resource_key);
            self.resource_cached
                .insert(icon.resource_key.clone(), icon.resource.clone());
            if let Some(waiters) = self.resource_waiters.remove(&icon.resource_key) {
                for waiter in waiters {
                    changed |= self.apply_loaded_resource(&waiter, &icon.resource);
                }
            }
        }
        changed
    }

    fn with_cached_render_image(&self, mut icon: FileIconSnapshot) -> FileIconSnapshot {
        if icon.render_image.is_none()
            && let Some(path) = icon.path.as_deref()
            && let Some(Some(render_image)) = self
                .render_images
                .lock()
                .expect("icon render image cache poisoned")
                .get(path)
        {
            icon.render_image = Some(Arc::clone(render_image));
        }
        icon
    }

    fn queue_icon_resource(&mut self, key: FileIconLoadKey, resource_key: IconResourceCacheKey) {
        if let Some(resource) = self.resource_cached.get(&resource_key).cloned() {
            self.apply_loaded_resource(&key, &resource);
            return;
        }
        let waiters = self
            .resource_waiters
            .entry(resource_key.clone())
            .or_default();
        if !waiters.iter().any(|waiter| waiter == &key) {
            waiters.push(key);
        }
        if !self.pending_load_keys.insert(resource_key.clone()) {
            return;
        }
        self.pending_loads
            .push_back(FileIconLoadRequest { resource_key });
    }

    fn apply_loaded_resource(
        &mut self,
        key: &FileIconLoadKey,
        resource: &FileIconResolvedResource,
    ) -> bool {
        match key {
            FileIconLoadKey::Named(key) => self
                .named_cached
                .get_mut(&key)
                .is_some_and(|icon| apply_loaded_resource_to_snapshot(icon, resource)),
            FileIconLoadKey::Role(key) => self
                .role_cached
                .get_mut(&key)
                .is_some_and(|icon| apply_loaded_resource_to_snapshot(icon, resource)),
        }
    }
}

#[derive(Clone, Debug)]
struct IconThemeResolver {
    roots: Vec<PathBuf>,
    themes: Vec<String>,
    search_order: Option<Vec<String>>,
    inherits_cache: HashMap<String, Vec<String>>,
    path_cache: HashMap<(String, u16), Option<PathBuf>>,
    directory_cache: HashMap<PathBuf, Option<IconDirectoryIndex>>,
}

#[derive(Clone, Debug, Default)]
struct IconDirectoryIndex {
    icons: HashMap<String, PathBuf>,
}

impl Default for IconThemeResolver {
    fn default() -> Self {
        Self {
            roots: icon_theme_roots(),
            themes: icon_theme_names(),
            search_order: None,
            inherits_cache: HashMap::new(),
            path_cache: HashMap::new(),
            directory_cache: HashMap::new(),
        }
    }
}

impl IconThemeResolver {
    fn find(&mut self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        let key = (icon_name.to_string(), desired_size);
        if let Some(path) = self.path_cache.get(&key) {
            return path.clone();
        }

        let path = self.find_uncached(icon_name, desired_size);
        self.path_cache.insert(key, path.clone());
        path
    }

    fn first_existing(
        &mut self,
        icon_names: &[String],
        desired_size: u16,
    ) -> Option<(String, PathBuf)> {
        icon_names.iter().find_map(|name| {
            self.find(name, desired_size)
                .map(|path| (name.clone(), path))
        })
    }

    fn find_uncached(&mut self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        let roots = self.roots.clone();
        for theme in self.theme_search_order() {
            for root in &roots {
                let theme_root = root.join(&theme);
                if let Some(path) = self.find_icon_in_theme(&theme_root, icon_name, desired_size) {
                    return Some(path);
                }
            }
        }

        [
            Path::new("/usr/share/pixmaps"),
            Path::new("/usr/local/share/pixmaps"),
        ]
        .into_iter()
        .find_map(|root| self.find_icon_direct(root, icon_name))
    }

    fn find_icon_in_theme(
        &mut self,
        theme_root: &Path,
        icon_name: &str,
        desired_size: u16,
    ) -> Option<PathBuf> {
        const CATEGORIES: &[&str] = &[
            "places",
            "mimetypes",
            "apps",
            "actions",
            "devices",
            "status",
        ];
        if self.icon_directory_index(theme_root).is_none() {
            return None;
        }
        if let Some(path) = self.find_icon_direct(theme_root, icon_name) {
            return Some(path);
        }
        for size in preferred_icon_size_dirs(desired_size) {
            for category in CATEGORIES {
                for base in [
                    theme_root.join(&size).join(category),
                    theme_root.join(category).join(&size),
                ] {
                    if let Some(path) = self.find_icon_direct(&base, icon_name) {
                        return Some(path);
                    }
                }
            }
        }
        for category in CATEGORIES {
            if let Some(path) = self.find_icon_direct(&theme_root.join(category), icon_name) {
                return Some(path);
            }
        }
        None
    }

    fn find_icon_direct(&mut self, root: &Path, icon_name: &str) -> Option<PathBuf> {
        self.icon_directory_index(root)
            .and_then(|index| index.icons.get(icon_name).cloned())
    }

    fn icon_directory_index(&mut self, root: &Path) -> Option<&IconDirectoryIndex> {
        if !self.directory_cache.contains_key(root) {
            let index = root.is_dir().then(|| IconDirectoryIndex::read(root));
            self.directory_cache.insert(root.to_path_buf(), index);
        }
        self.directory_cache.get(root).and_then(Option::as_ref)
    }

    fn theme_search_order(&mut self) -> Vec<String> {
        if let Some(search_order) = &self.search_order {
            return search_order.clone();
        }
        let mut themes = Vec::new();
        for theme in self.themes.clone() {
            self.push_theme_and_inherits(theme, &mut themes, 0);
        }
        self.search_order = Some(themes.clone());
        themes
    }

    fn push_theme_and_inherits(&mut self, theme: String, themes: &mut Vec<String>, depth: usize) {
        if depth > 8 || theme.is_empty() {
            return;
        }
        let already_seen = themes.iter().any(|existing| existing == &theme);
        push_unique_icon_theme(themes, &theme);
        if already_seen {
            return;
        }
        for inherited in self.inherited_themes(&theme) {
            self.push_theme_and_inherits(inherited, themes, depth + 1);
        }
    }

    fn inherited_themes(&mut self, theme: &str) -> Vec<String> {
        if let Some(inherited) = self.inherits_cache.get(theme) {
            return inherited.clone();
        }
        let mut inherited = Vec::new();
        for root in &self.roots {
            let Ok(contents) = fs::read_to_string(root.join(theme).join("index.theme")) else {
                continue;
            };
            for theme in parse_icon_theme_inherits(&contents) {
                push_unique_icon_theme(&mut inherited, &theme);
            }
        }
        self.inherits_cache
            .insert(theme.to_string(), inherited.clone());
        inherited
    }
}

impl IconDirectoryIndex {
    fn read(root: &Path) -> Self {
        let mut icons = HashMap::<String, (u8, PathBuf)>::new();
        let Ok(entries) = fs::read_dir(root) else {
            return Self::default();
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some((name, extension_rank)) = icon_file_name_and_rank(&path) else {
                continue;
            };
            let file_type = entry.file_type().ok();
            if !file_type.is_some_and(|file_type| file_type.is_file() || file_type.is_symlink()) {
                continue;
            }
            match icons.get(&name) {
                Some((existing_rank, _)) if *existing_rank <= extension_rank => {}
                _ => {
                    icons.insert(name, (extension_rank, path));
                }
            }
        }

        Self {
            icons: icons
                .into_iter()
                .map(|(name, (_, path))| (name, path))
                .collect(),
        }
    }
}

fn file_icon_kind(path: &Path, is_dir: bool, mime_type: Option<Arc<str>>) -> FileIconKind {
    if is_dir {
        return FileIconKind::Directory;
    }
    let extension = file_extension(path);
    match mime_type {
        Some(mime) => FileIconKind::Mime { mime, extension },
        None => FileIconKind::File { extension },
    }
}

fn file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn icon_cache_size(icon_size: f32) -> u16 {
    icon_size.round().clamp(16.0, 256.0) as u16
}

fn absolute_icon_candidate(icon_name: &str) -> Option<PathBuf> {
    let path = Path::new(icon_name);
    if path.is_absolute() && has_renderable_icon_extension(path) {
        return Some(path.to_path_buf());
    }
    None
}

fn file_icon_role_name(kind: &FileIconKind, mime: &fika_core::MimeDatabase) -> String {
    let profile = file_icon_profile(kind, mime);
    profile
        .icon_candidates
        .first()
        .or_else(|| profile.generic_candidates.first())
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

fn file_icon_snapshot_from_resolved_path(
    icon_name: String,
    path: Option<PathBuf>,
    fallback_marker: String,
    fallback_fg: u32,
    fallback_bg: u32,
) -> FileIconSnapshot {
    FileIconSnapshot {
        icon_name: Arc::from(icon_name),
        path: path.map(|path| Arc::from(path.into_boxed_path())),
        render_image: None,
        fallback_marker: Arc::from(fallback_marker),
        fallback_fg,
        fallback_bg,
    }
}

fn apply_loaded_resource_to_snapshot(
    icon: &mut FileIconSnapshot,
    resource: &FileIconResolvedResource,
) -> bool {
    let new_path = resource
        .path
        .clone()
        .map(|path| Arc::from(path.into_boxed_path()));
    let name_changed = icon.icon_name.as_ref() != resource.icon_name;
    let path_changed = icon.path.as_deref() != new_path.as_deref();
    let image_changed = match (&icon.render_image, &resource.render_image) {
        (Some(left), Some(right)) => !Arc::ptr_eq(left, right),
        (None, None) => false,
        _ => true,
    };

    if name_changed {
        icon.icon_name = Arc::from(resource.icon_name.as_str());
    }
    if path_changed {
        icon.path = new_path;
    }
    if image_changed {
        icon.render_image = resource.render_image.clone();
    }

    name_changed || path_changed || image_changed
}

fn resolve_icon_load_request(
    resolver: &Arc<Mutex<IconThemeResolver>>,
    candidates: &[String],
    size_px: u16,
) -> (String, Option<PathBuf>) {
    if let Some((name, path)) = candidates.iter().find_map(|candidate| {
        absolute_icon_candidate(candidate).map(|path| (candidate.clone(), path))
    }) {
        return (name, Some(path));
    }

    if let Some((name, path)) = resolver
        .lock()
        .expect("icon theme resolver poisoned")
        .first_existing(candidates, size_px)
    {
        return (name, Some(path));
    }

    (
        candidates
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        None,
    )
}

fn cached_render_image(
    cache: &Arc<Mutex<HashMap<PathBuf, Option<Arc<RenderImage>>>>>,
    path: &Path,
) -> Option<Option<Arc<RenderImage>>> {
    cache
        .lock()
        .expect("icon render image cache poisoned")
        .get(path)
        .cloned()
}

fn load_icon_render_image(path: &Path) -> Option<Arc<RenderImage>> {
    let format = icon_image_format(path)?;
    let bytes = fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Image::from_bytes(format, bytes)
        .to_image_data(SvgRenderer::new(Arc::new(())))
        .ok()
}

fn icon_image_format(path: &Path) -> Option<ImageFormat> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some(ImageFormat::Png),
        Some("jpg" | "jpeg") => Some(ImageFormat::Jpeg),
        Some("webp") => Some(ImageFormat::Webp),
        Some("gif") => Some(ImageFormat::Gif),
        Some("svg") => Some(ImageFormat::Svg),
        Some("bmp") => Some(ImageFormat::Bmp),
        Some("tif" | "tiff") => Some(ImageFormat::Tiff),
        Some("ico") => Some(ImageFormat::Ico),
        Some("pbm" | "pgm" | "ppm" | "pnm") => Some(ImageFormat::Pnm),
        _ => None,
    }
}

struct FileIconProfile {
    icon_candidates: Vec<String>,
    generic_candidates: Vec<String>,
    marker: String,
    fg: u32,
    bg: u32,
}

fn file_icon_profile(kind: &FileIconKind, mime: &fika_core::MimeDatabase) -> FileIconProfile {
    let (icon_candidates, generic_candidates, marker, fg, bg) = match kind {
        FileIconKind::Directory => (
            vec!["folder".to_string(), "inode-directory".to_string()],
            Vec::new(),
            "DIR".to_string(),
            0x0f4c81,
            0xe7f1fb,
        ),
        FileIconKind::Mime {
            mime: mime_name,
            extension,
        } => {
            let marker = file_marker(mime_name, extension.as_deref());
            let (fg, bg) = file_fallback_colors(mime_name, extension.as_deref());
            (
                mime_icon_candidates(mime_name, mime),
                mime_generic_icon_candidates(mime_name, mime),
                marker,
                fg,
                bg,
            )
        }
        FileIconKind::File { extension } => {
            let marker = file_marker("application/octet-stream", extension.as_deref());
            let (fg, bg) = file_fallback_colors("application/octet-stream", extension.as_deref());
            (
                fallback_file_icon_candidates(),
                mime_generic_icon_candidates("application/octet-stream", mime),
                marker,
                fg,
                bg,
            )
        }
    };

    FileIconProfile {
        icon_candidates,
        generic_candidates,
        marker,
        fg,
        bg,
    }
}

fn mime_icon_candidates(mime_name: &str, mime: &fika_core::MimeDatabase) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Some(icon_name) = mime.icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    if let Some(icon_name) = fika_core::mime_icon_name(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn fallback_file_icon_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    push_icon_candidate(&mut candidates, "application-octet-stream");
    candidates
}

fn mime_generic_icon_candidates(mime_name: &str, mime: &fika_core::MimeDatabase) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(icon_name) = mime.generic_icon_name_for_mime(mime_name) {
        push_icon_candidate(&mut candidates, icon_name);
    }
    candidates
}

fn push_icon_candidate(candidates: &mut Vec<String>, icon_name: impl Into<String>) {
    let icon_name = icon_name.into();
    if !candidates.iter().any(|existing| existing == &icon_name) {
        candidates.push(icon_name);
    }
}

fn file_marker(mime: &str, extension: Option<&str>) -> String {
    match extension {
        Some(extension) if extension.len() <= 4 => extension.to_ascii_uppercase(),
        _ if mime.starts_with("image/") => "IMG".to_string(),
        _ if mime.starts_with("audio/") => "AUD".to_string(),
        _ if mime.starts_with("video/") => "VID".to_string(),
        _ if mime.starts_with("text/") => "TXT".to_string(),
        _ => "FILE".to_string(),
    }
}

fn file_fallback_colors(mime: &str, extension: Option<&str>) -> (u32, u32) {
    if mime.starts_with("image/") || extension.is_some_and(is_image_extension) {
        (0x7c2d12, 0xffedd5)
    } else if mime.starts_with("audio/") || extension.is_some_and(is_audio_extension) {
        (0x6d28d9, 0xf3e8ff)
    } else if mime.starts_with("video/") || extension.is_some_and(is_video_extension) {
        (0x9f1239, 0xffe4e6)
    } else if extension.is_some_and(is_archive_extension) {
        (0x713f12, 0xfef3c7)
    } else if mime == "application/pdf" || extension == Some("pdf") {
        (0x991b1b, 0xfee2e2)
    } else {
        (0x374151, 0xf3f4f6)
    }
}

fn icon_theme_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(home).join(".local/share/icons"));
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(data_home).join("icons"));
    }

    let data_dirs =
        env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        push_unique_icon_path(&mut roots, Path::new(dir).join("icons"));
    }
    push_unique_icon_path(&mut roots, PathBuf::from("/usr/share/icons"));
    roots
}

fn icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for theme in configured_icon_theme_names() {
        push_unique_icon_theme(&mut themes, &theme);
    }
    if env::var_os("KDE_FULL_SESSION").is_some()
        || env::var("XDG_CURRENT_DESKTOP")
            .map(|desktop| desktop.to_ascii_lowercase().contains("kde"))
            .unwrap_or(false)
    {
        push_unique_icon_theme(&mut themes, "breeze");
        push_unique_icon_theme(&mut themes, "breeze-dark");
    }
    for key in [
        "GTK_THEME",
        "ICON_THEME",
        "DESKTOP_SESSION",
        "XDG_CURRENT_DESKTOP",
    ] {
        if let Ok(value) = env::var(key) {
            for part in value.split([':', ';']) {
                let theme = part.trim();
                if !theme.is_empty() {
                    push_unique_icon_theme(&mut themes, theme);
                }
            }
        }
    }
    for fallback in [
        "breeze",
        "breeze-dark",
        "Papirus",
        "Papirus-Dark",
        "Papirus-Light",
        "Adwaita",
        "hicolor",
    ] {
        push_unique_icon_theme(&mut themes, fallback);
    }
    themes
}

fn configured_icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for path in icon_theme_config_paths() {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for theme in parse_configured_icon_theme_names(&contents) {
            push_unique_icon_theme(&mut themes, &theme);
        }
    }
    themes
}

fn icon_theme_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        let config_home = PathBuf::from(config_home);
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
    }
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        let home = PathBuf::from(home);
        let config_home = home.join(".config");
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
        push_unique_icon_path(&mut paths, home.join(".gtkrc-2.0"));
    }
    paths
}

fn parse_configured_icon_theme_names(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    let mut in_icons_section = false;
    let mut in_icon_theme_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            in_icons_section = section.eq_ignore_ascii_case("Icons");
            in_icon_theme_section = section.eq_ignore_ascii_case("Icon Theme");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.eq_ignore_ascii_case("gtk-icon-theme-name")
            || (in_icons_section && key.eq_ignore_ascii_case("Theme"))
            || (in_icon_theme_section && key.eq_ignore_ascii_case("Name"))
        {
            let theme = value.trim().trim_matches('"');
            if !theme.is_empty() {
                push_unique_icon_theme(&mut themes, theme);
            }
        }
    }
    themes
}

fn parse_icon_theme_inherits(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "Inherits" {
            continue;
        }
        for theme in value
            .split(',')
            .map(str::trim)
            .filter(|theme| !theme.is_empty())
        {
            push_unique_icon_theme(&mut themes, theme);
        }
    }
    themes
}

#[cfg(test)]
fn find_icon_in_theme(theme_root: &Path, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
    const CATEGORIES: &[&str] = &[
        "places",
        "mimetypes",
        "apps",
        "actions",
        "devices",
        "status",
    ];
    let mut directory_cache = HashMap::new();
    if find_icon_directory_index(theme_root, &mut directory_cache).is_none() {
        return None;
    }
    if let Some(path) = find_icon_direct_cached(theme_root, icon_name, &mut directory_cache) {
        return Some(path);
    }
    for size in preferred_icon_size_dirs(desired_size) {
        for category in CATEGORIES {
            for base in [
                theme_root.join(&size).join(category),
                theme_root.join(category).join(&size),
            ] {
                if let Some(path) = find_icon_direct_cached(&base, icon_name, &mut directory_cache)
                {
                    return Some(path);
                }
            }
        }
    }
    for category in CATEGORIES {
        if let Some(path) =
            find_icon_direct_cached(&theme_root.join(category), icon_name, &mut directory_cache)
        {
            return Some(path);
        }
    }
    None
}

fn preferred_icon_size_dirs(desired_size: u16) -> Vec<String> {
    let mut dirs = Vec::new();
    let fixed_sizes = [256u16, 128, 96, 64, 48, 32, 24, 22, 16];
    let desired = desired_size.max(16);
    let mut ordered = fixed_sizes.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|size| size.abs_diff(desired));
    for size in ordered {
        push_icon_size_dir(&mut dirs, format!("{size}x{size}"));
        push_icon_size_dir(&mut dirs, size.to_string());
    }
    push_icon_size_dir(&mut dirs, "scalable".to_string());
    push_icon_size_dir(&mut dirs, "symbolic".to_string());
    dirs
}

fn push_icon_size_dir(dirs: &mut Vec<String>, value: String) {
    if !dirs.iter().any(|existing| existing == &value) {
        dirs.push(value);
    }
}

#[cfg(test)]
fn find_icon_direct_cached(
    root: &Path,
    icon_name: &str,
    directory_cache: &mut HashMap<PathBuf, Option<IconDirectoryIndex>>,
) -> Option<PathBuf> {
    find_icon_directory_index(root, directory_cache)
        .and_then(|index| index.icons.get(icon_name).cloned())
}

#[cfg(test)]
fn find_icon_directory_index<'a>(
    root: &Path,
    directory_cache: &'a mut HashMap<PathBuf, Option<IconDirectoryIndex>>,
) -> Option<&'a IconDirectoryIndex> {
    if !directory_cache.contains_key(root) {
        directory_cache.insert(
            root.to_path_buf(),
            root.is_dir().then(|| IconDirectoryIndex::read(root)),
        );
    }
    directory_cache.get(root).and_then(Option::as_ref)
}

fn icon_file_name_and_rank(path: &Path) -> Option<(String, u8)> {
    let extension = path.extension()?.to_str()?;
    let rank = renderable_icon_extension_rank(extension)?;
    let name = path.file_stem()?.to_str()?.to_string();
    (!name.is_empty()).then_some((name, rank))
}

fn renderable_icon_extension_rank(extension: &str) -> Option<u8> {
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some(0),
        "svg" => Some(1),
        "webp" => Some(2),
        "jpg" => Some(3),
        "jpeg" => Some(4),
        "bmp" => Some(5),
        "gif" => Some(6),
        "ico" => Some(7),
        _ => None,
    }
}

fn has_renderable_icon_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "svg" | "webp" | "jpg" | "jpeg" | "bmp" | "gif" | "ico")
    )
}

fn push_unique_icon_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn push_unique_icon_theme(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn is_image_extension(extension: &str) -> bool {
    matches!(
        extension,
        "avif" | "bmp" | "gif" | "heic" | "jpeg" | "jpg" | "png" | "svg" | "tif" | "tiff" | "webp"
    )
}

fn is_archive_extension(extension: &str) -> bool {
    matches!(
        extension,
        "7z" | "bz2" | "gz" | "rar" | "tar" | "xz" | "zip" | "zst"
    )
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension,
        "aac" | "flac" | "m4a" | "mp3" | "ogg" | "opus" | "wav"
    )
}

fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension,
        "avi" | "m4v" | "mkv" | "mov" | "mp4" | "mpeg" | "mpg" | "webm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENERIC_BINARY_MIME: &str = "application/octet-stream";

    fn test_icon_cache(root: &Path) -> FileIconCache {
        FileIconCache {
            named_cached: HashMap::new(),
            role_cached: HashMap::new(),
            resource_cached: HashMap::new(),
            render_images: Arc::new(Mutex::new(HashMap::new())),
            pending_load_keys: HashSet::new(),
            pending_loads: VecDeque::new(),
            resource_waiters: HashMap::new(),
            icon_load_batch_pending: false,
            theme: Arc::new(Mutex::new(IconThemeResolver {
                roots: vec![root.to_path_buf()],
                themes: vec!["theme".to_string()],
                search_order: None,
                inherits_cache: HashMap::new(),
                path_cache: HashMap::new(),
                directory_cache: HashMap::new(),
            })),
            mime: fika_core::MimeDatabase::from_maps(
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
            ),
        }
    }

    fn drain_icon_loads(cache: &mut FileIconCache) -> bool {
        let mut changed = false;
        while let Some(batch) = cache.take_icon_load_batch(64) {
            changed |= cache.finish_icon_load_batch(FileIconCache::load_icon_batch(batch));
        }
        changed
    }

    #[test]
    fn mime_icon_candidates_keep_specific_text_icon_before_generic_text() {
        let mime = fika_core::MimeDatabase::from_maps(
            HashMap::new(),
            HashMap::from([("text/rust".to_string(), "text-x-rust".to_string())]),
            HashMap::from([("text/rust".to_string(), "text-x-source".to_string())]),
        );

        assert_eq!(
            mime_icon_candidates("text/rust", &mime),
            &["text-x-rust".to_string(), "text-rust".to_string()]
        );
        assert_eq!(
            mime_icon_candidates("text/plain", &mime),
            &["text-plain".to_string()]
        );
        assert_eq!(
            mime_icon_candidates(GENERIC_BINARY_MIME, &mime),
            &["application-octet-stream".to_string()]
        );
        assert_eq!(
            mime_generic_icon_candidates("text/rust", &mime),
            &["text-x-source".to_string()]
        );
    }

    #[test]
    fn text_plain_and_generic_binary_do_not_use_extension_specific_guesses() {
        let mime =
            fika_core::MimeDatabase::from_maps(HashMap::new(), HashMap::new(), HashMap::new());

        let generic_icon_name = file_icon_role_name(
            &FileIconKind::Mime {
                mime: Arc::from(GENERIC_BINARY_MIME),
                extension: Some("conf".to_string()),
            },
            &mime,
        );
        let text_icon_name = file_icon_role_name(
            &FileIconKind::Mime {
                mime: Arc::from("text/plain"),
                extension: Some("conf".to_string()),
            },
            &mime,
        );

        assert_eq!(generic_icon_name, "application-octet-stream");
        assert_eq!(text_icon_name, "text-plain");
    }

    #[test]
    fn preferred_icon_size_dirs_prioritize_nearest_size() {
        let dirs = preferred_icon_size_dirs(40);

        assert_eq!(dirs[0], "48x48");
        assert_eq!(dirs[1], "48");
        assert_eq!(dirs[2], "32x32");
        assert!(dirs.iter().position(|dir| dir == "16x16").unwrap() > 4);
        assert!(dirs.contains(&"scalable".to_string()));
    }

    #[test]
    fn icon_theme_inherits_are_parsed_from_index_theme() {
        assert_eq!(
            parse_icon_theme_inherits(
                "\
[Icon Theme]\n\
Name=Child\n\
Inherits=parent-one, parent-two ,hicolor\n"
            ),
            vec![
                "parent-one".to_string(),
                "parent-two".to_string(),
                "hicolor".to_string()
            ]
        );
    }

    #[test]
    fn configured_icon_themes_parse_kde_and_gtk_settings() {
        assert_eq!(
            parse_configured_icon_theme_names(
                "\
[Icons]\n\
Theme=Papirus\n\
\n\
[Settings]\n\
gtk-icon-theme-name=breeze\n"
            ),
            vec!["Papirus".to_string(), "breeze".to_string()]
        );
        assert_eq!(
            parse_configured_icon_theme_names("gtk-icon-theme-name=\"Papirus-Dark\"\n"),
            vec!["Papirus-Dark".to_string()]
        );
    }

    #[test]
    fn icon_theme_resolver_searches_inherited_themes() {
        let root = test_dir("icon-theme-inherits");
        std::fs::create_dir_all(root.join("child")).unwrap();
        std::fs::create_dir_all(root.join("parent/48x48/mimetypes")).unwrap();
        std::fs::write(root.join("child/index.theme"), "Inherits=parent\n").unwrap();
        std::fs::write(
            root.join("parent/48x48/mimetypes/text-rust.svg"),
            test_svg(),
        )
        .unwrap();
        let mut resolver = IconThemeResolver {
            roots: vec![root.clone()],
            themes: vec!["child".to_string()],
            search_order: None,
            inherits_cache: HashMap::new(),
            path_cache: HashMap::new(),
            directory_cache: HashMap::new(),
        };

        assert_eq!(
            resolver.find("text-rust", 40),
            Some(root.join("parent/48x48/mimetypes/text-rust.svg"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn find_icon_in_theme_supports_breeze_category_size_layout() {
        let root = test_dir("icon-theme-breeze-layout");
        std::fs::create_dir_all(root.join("mimetypes/32")).unwrap();
        std::fs::write(root.join("mimetypes/32/text-rust.svg"), test_svg()).unwrap();

        assert_eq!(
            find_icon_in_theme(&root, "text-rust", 40),
            Some(root.join("mimetypes/32/text-rust.svg"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn find_icon_in_theme_chooses_nearest_requested_size() {
        let root = test_dir("icon-theme-size");
        std::fs::create_dir_all(root.join("32x32/mimetypes")).unwrap();
        std::fs::create_dir_all(root.join("48x48/mimetypes")).unwrap();
        std::fs::write(root.join("32x32/mimetypes/text-rust.svg"), test_svg()).unwrap();
        std::fs::write(root.join("48x48/mimetypes/text-rust.svg"), test_svg()).unwrap();

        assert_eq!(
            find_icon_in_theme(&root, "text-rust", 40),
            Some(root.join("48x48/mimetypes/text-rust.svg"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn icon_name_role_is_rendered_without_recomputing_from_mime() {
        let root = test_dir("icon-role-cache");
        std::fs::create_dir_all(root.join("theme/48x48/mimetypes")).unwrap();
        std::fs::write(
            root.join("theme/48x48/mimetypes/application-octet-stream.svg"),
            test_svg(),
        )
        .unwrap();
        std::fs::write(
            root.join("theme/48x48/mimetypes/text-plain.svg"),
            test_svg(),
        )
        .unwrap();
        let mut cache = test_icon_cache(&root);

        let role = cache.icon_name_for(
            Path::new("settings.conf"),
            false,
            Some(Arc::from(GENERIC_BINARY_MIME)),
        );
        let rendered = cache.icon_for_name_role(
            role.as_ref(),
            Path::new("settings.conf"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );

        assert_eq!(role.as_ref(), "application-octet-stream");
        assert_eq!(rendered.icon_name.as_ref(), "application-octet-stream");
        assert!(rendered.render_image.is_none());
        assert!(drain_icon_loads(&mut cache));
        let rendered = cache.icon_for_name_role(
            role.as_ref(),
            Path::new("settings.conf"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );
        assert!(rendered.render_image.is_some());
        assert_eq!(cache.role_cached.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn named_icon_cache_returns_loaded_render_image() {
        let root = test_dir("named-icon-render-cache");
        std::fs::create_dir_all(root.join("theme/48x48/actions")).unwrap();
        std::fs::write(
            root.join("theme/48x48/actions/archive-insert.svg"),
            test_svg(),
        )
        .unwrap();
        let mut cache = test_icon_cache(&root);

        let icon = cache.named_icon(
            "archive-insert",
            &["archive-insert"],
            "S",
            0x0f766e,
            0xe6fffb,
            18.0,
        );

        assert_eq!(icon.icon_name.as_ref(), "archive-insert");
        assert!(icon.render_image.is_none());
        assert!(drain_icon_loads(&mut cache));
        let icon = cache.named_icon(
            "archive-insert",
            &["archive-insert"],
            "S",
            0x0f766e,
            0xe6fffb,
            18.0,
        );
        assert!(icon.render_image.is_some());
        assert_eq!(cache.named_cached.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn icon_snapshot_queues_only_one_background_load_per_cache_key() {
        let root = test_dir("icon-load-dedupe");
        std::fs::create_dir_all(root.join("theme/48x48/mimetypes")).unwrap();
        std::fs::write(
            root.join("theme/48x48/mimetypes/text-plain.svg"),
            test_svg(),
        )
        .unwrap();
        let mut cache = test_icon_cache(&root);

        let first = cache.icon_for_name_role(
            "text-plain",
            Path::new("a.txt"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );
        let second = cache.icon_for_name_role(
            "text-plain",
            Path::new("b.txt"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );

        assert_eq!(first.icon_name, second.icon_name);
        assert!(first.render_image.is_none());
        assert!(second.render_image.is_none());
        let batch = cache.take_icon_load_batch(64).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(cache.take_icon_load_batch(64).is_none());
        assert!(cache.finish_icon_load_batch(FileIconCache::load_icon_batch(batch)));

        let loaded = cache.icon_for_name_role(
            "text-plain",
            Path::new("a.txt"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );
        assert!(loaded.render_image.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn icon_resource_load_is_shared_across_role_fallback_styles() {
        let root = test_dir("icon-resource-load-dedupe");
        std::fs::create_dir_all(root.join("theme/48x48/mimetypes")).unwrap();
        std::fs::write(
            root.join("theme/48x48/mimetypes/text-plain.svg"),
            test_svg(),
        )
        .unwrap();
        let mut cache = test_icon_cache(&root);

        let first = cache.icon_for_name_role(
            "text-plain",
            Path::new("notes.txt"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );
        let second = cache.icon_for_name_role(
            "text-plain",
            Path::new("settings.conf"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );

        assert_ne!(first.fallback_marker, second.fallback_marker);
        assert_eq!(cache.role_cached.len(), 2);
        let batch = cache.take_icon_load_batch(64).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(cache.finish_icon_load_batch(FileIconCache::load_icon_batch(batch)));

        let first = cache.icon_for_name_role(
            "text-plain",
            Path::new("notes.txt"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );
        let second = cache.icon_for_name_role(
            "text-plain",
            Path::new("settings.conf"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );
        assert!(first.render_image.is_some());
        assert!(second.render_image.is_some());
        assert_eq!(cache.resource_cached.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn icon_preload_queues_resource_without_visible_snapshot() {
        let root = test_dir("icon-preload-role");
        std::fs::create_dir_all(root.join("theme/48x48/mimetypes")).unwrap();
        std::fs::write(
            root.join("theme/48x48/mimetypes/text-plain.svg"),
            test_svg(),
        )
        .unwrap();
        let mut cache = test_icon_cache(&root);

        cache.preload_icon_for_model_role(
            None,
            Path::new("notes.txt"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );

        assert_eq!(cache.role_cached.len(), 1);
        let batch = cache.take_icon_load_batch(64).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(cache.finish_icon_load_batch(FileIconCache::load_icon_batch(batch)));
        let rendered = cache.icon_for_name_role(
            "text-plain",
            Path::new("notes.txt"),
            false,
            Some(Arc::from("text/plain")),
            48.0,
        );
        assert!(rendered.render_image.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn named_icon_accepts_desktop_absolute_icon_path() {
        let root = test_dir("named-icon-absolute-path");
        std::fs::create_dir_all(&root).unwrap();
        let icon_path = root.join("service-icon.svg");
        std::fs::write(&icon_path, test_svg()).unwrap();
        let icon_name = icon_path.to_string_lossy().into_owned();
        let mut cache = test_icon_cache(&root);

        let icon = cache.named_icon(
            &icon_name,
            &[&icon_name, "application-x-executable"],
            "S",
            0x0f766e,
            0xe6fffb,
            18.0,
        );

        assert_eq!(icon.icon_name.as_ref(), icon_name);
        assert_eq!(icon.path, None);
        assert!(icon.render_image.is_none());
        assert!(drain_icon_loads(&mut cache));
        let icon = cache.named_icon(
            &icon_name,
            &[&icon_name, "application-x-executable"],
            "S",
            0x0f766e,
            0xe6fffb,
            18.0,
        );
        assert_eq!(icon.path.as_deref(), Some(icon_path.as_path()));
        assert!(icon.render_image.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_resolved_icon_path_keeps_fallback_without_panicking() {
        let root = test_dir("icon-cache-missing-image");
        let missing = root.join("theme/48x48/mimetypes/text-rust.svg");

        let icon = file_icon_snapshot_from_resolved_path(
            "text-rust".to_string(),
            Some(missing.clone()),
            "TXT".to_string(),
            0x374151,
            0xf3f4f6,
        );

        assert_eq!(icon.path.as_deref(), Some(missing.as_path()));
        assert!(icon.render_image.is_none());
        assert_eq!(icon.fallback_marker.as_ref(), "TXT");
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("fika-icons-{name}-{}-{nanos}", std::process::id()))
    }

    fn test_svg() -> &'static str {
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48"><rect width="48" height="48" fill="#2f6fed"/></svg>"##
    }
}
