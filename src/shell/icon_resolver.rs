use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, Sender},
};
use std::thread;

use crate::wgpu_icon_roles::{
    FileIconKind, FileIconPathCacheKey, FileIconProfile, FileIconRoleCacheKey, NamedIconFallback,
    file_icon_path_cache_key, file_icon_profile, icon_cache_size,
};
use crate::{Entry, IconThemeResolver, file_icon_snapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedFileIcon {
    pub(crate) path: Option<PathBuf>,
}

pub(crate) struct FileIconResolver {
    cached: HashMap<FileIconPathCacheKey, ResolvedFileIcon>,
    pending: HashSet<FileIconPathCacheKey>,
    fast_theme: IconThemeResolver,
    fast_profiles: HashMap<FileIconRoleCacheKey, FileIconProfile>,
    request_tx: Option<Sender<IconResolveRequest>>,
    result_rx: Receiver<IconResolveResult>,
}

const DOLPHIN_VISIBLE_ICON_PREWARM_SIZES: &[u16] = &[16, 22, 32, 48, 64, 80, 96, 112, 128, 144];

#[derive(Clone, Debug)]
struct IconResolveRequest {
    key: FileIconPathCacheKey,
}

#[derive(Clone, Debug)]
struct IconResolveResult {
    key: FileIconPathCacheKey,
    icon: ResolvedFileIcon,
}

impl FileIconResolver {
    pub(crate) fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<IconResolveRequest>();
        let (result_tx, result_rx) = mpsc::channel::<IconResolveResult>();
        let request_tx = thread::Builder::new()
            .name("fika-wgpu-icon-resolver".to_string())
            .spawn(move || icon_resolve_worker(request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        let mut resolver = Self {
            cached: HashMap::new(),
            pending: HashSet::new(),
            fast_theme: IconThemeResolver::default(),
            fast_profiles: HashMap::new(),
            request_tx,
            result_rx,
        };
        resolver.prewarm_common_visible_roles();
        resolver
    }

    fn prewarm_common_visible_roles(&mut self) {
        let roles = [
            FileIconKind::Directory,
            FileIconKind::File { extension: None },
            FileIconKind::PreliminaryFile { extension: None },
            FileIconKind::Mime {
                mime: Arc::from(fika_core::GENERIC_BINARY_MIME),
            },
            FileIconKind::Mime {
                mime: Arc::from("text/plain"),
            },
        ];

        for size_px in DOLPHIN_VISIBLE_ICON_PREWARM_SIZES {
            for kind in roles.iter().cloned() {
                self.resolve_key_fast(FileIconPathCacheKey {
                    role: FileIconRoleCacheKey { kind },
                    size_px: *size_px,
                });
            }
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resolve_entry(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> Option<ResolvedFileIcon> {
        self.drain_results();
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            icon_size,
        );
        self.resolve_key(key)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resolve_entry_fast(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> ResolvedFileIcon {
        self.drain_results();
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            icon_size,
        );
        self.resolve_key_fast(key)
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_entry_visible(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> (ResolvedFileIcon, bool) {
        self.drain_results();
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            icon_size,
        );
        if let Some(icon) = self.resolve_key(key.clone()) {
            return (icon, false);
        }

        let fallback_key = visible_icon_fallback_key(&key);
        if let Some(icon) = self.cached.get(&fallback_key) {
            return (icon.clone(), true);
        }

        (self.resolve_key_fast(fallback_key), true)
    }

    pub(crate) fn resolve_named(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        icon_size: f32,
    ) -> Option<ResolvedFileIcon> {
        self.drain_results();
        let icon_name = icon_name.trim();
        if icon_name.is_empty() {
            return None;
        }
        let key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Named {
                    icon_name: icon_name.to_string(),
                    fallback,
                },
            },
            size_px: icon_cache_size(icon_size),
        };
        self.resolve_key(key)
    }

    pub(crate) fn resolve_path_cache_key_fast(
        &mut self,
        key: FileIconPathCacheKey,
    ) -> ResolvedFileIcon {
        self.drain_results();
        self.resolve_key_fast(key)
    }

    fn resolve_key(&mut self, key: FileIconPathCacheKey) -> Option<ResolvedFileIcon> {
        if let Some(icon) = self.cached.get(&key) {
            return Some(icon.clone());
        }

        if self.pending.insert(key.clone())
            && self
                .request_tx
                .as_ref()
                .is_none_or(|tx| tx.send(IconResolveRequest { key }).is_err())
        {
            self.pending.clear();
        }
        None
    }

    fn resolve_key_fast(&mut self, key: FileIconPathCacheKey) -> ResolvedFileIcon {
        if let Some(icon) = self.cached.get(&key) {
            return icon.clone();
        }

        let profile = self
            .fast_profiles
            .entry(key.role.clone())
            .or_insert_with(|| {
                file_icon_profile(&key.role.kind, fika_core::MimeDatabase::shared())
            });
        let icon = file_icon_snapshot(profile, key.size_px, &mut self.fast_theme);
        self.pending.remove(&key);
        self.cached.insert(key, icon.clone());
        icon
    }

    pub(crate) fn drain_results(&mut self) -> usize {
        let mut changed = 0usize;
        while let Ok(result) = self.result_rx.try_recv() {
            self.pending.remove(&result.key);
            self.cached.insert(result.key, result.icon);
            changed += 1;
        }
        changed
    }

    pub(crate) fn has_pending(&mut self) -> bool {
        self.drain_results();
        !self.pending.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn pending_len_for_test(&mut self) -> usize {
        self.drain_results();
        self.pending.len()
    }

    #[cfg(test)]
    pub(crate) fn cached_len_for_test(&mut self) -> usize {
        self.drain_results();
        self.cached.len()
    }
}

fn visible_icon_fallback_key(key: &FileIconPathCacheKey) -> FileIconPathCacheKey {
    let kind = match &key.role.kind {
        FileIconKind::Directory => FileIconKind::Directory,
        _ => FileIconKind::File { extension: None },
    };
    FileIconPathCacheKey {
        role: FileIconRoleCacheKey { kind },
        size_px: key.size_px,
    }
}

fn icon_resolve_worker(
    request_rx: Receiver<IconResolveRequest>,
    result_tx: Sender<IconResolveResult>,
) {
    let mut theme = IconThemeResolver::default();
    let mime = fika_core::MimeDatabase::shared();
    let mut roles = HashMap::<FileIconRoleCacheKey, FileIconProfile>::new();
    while let Ok(request) = request_rx.recv() {
        let profile = roles
            .entry(request.key.role.clone())
            .or_insert_with(|| file_icon_profile(&request.key.role.kind, mime));
        let icon = file_icon_snapshot(profile, request.key.size_px, &mut theme);
        if result_tx
            .send(IconResolveResult {
                key: request.key,
                icon,
            })
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
pub(crate) struct FileIconResolverTestHarness {
    pub(crate) resolver: FileIconResolver,
    request_rx: Receiver<IconResolveRequest>,
    result_tx: Sender<IconResolveResult>,
}

#[cfg(test)]
impl FileIconResolverTestHarness {
    pub(crate) fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<IconResolveRequest>();
        let (result_tx, result_rx) = mpsc::channel::<IconResolveResult>();
        Self {
            resolver: FileIconResolver {
                cached: HashMap::new(),
                pending: HashSet::new(),
                fast_theme: IconThemeResolver::default(),
                fast_profiles: HashMap::new(),
                request_tx: Some(request_tx),
                result_rx,
            },
            request_rx,
            result_tx,
        }
    }

    pub(crate) fn next_request_key(&mut self) -> Option<FileIconPathCacheKey> {
        self.request_rx.try_recv().ok().map(|request| request.key)
    }

    pub(crate) fn complete(&self, key: FileIconPathCacheKey, path: Option<PathBuf>) {
        let _ = self.result_tx.send(IconResolveResult {
            key,
            icon: ResolvedFileIcon { path },
        });
    }
}
