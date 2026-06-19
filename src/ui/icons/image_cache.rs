use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::FileIconSnapshot;

const UNKNOWN_THEME_NAME: &str = "__fika_unknown_icon_theme__";
const DEFAULT_THEME_ICON_READINESS_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum IconColorScheme {
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum IconPaintMode {
    Normal,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ThemeIconImageKey {
    pub(crate) icon_name: Arc<str>,
    pub(crate) icon_size_px: u32,
    pub(crate) scale_bits: u32,
    pub(crate) theme_name: Arc<str>,
    pub(crate) color_scheme: IconColorScheme,
    pub(crate) mode: IconPaintMode,
}

impl ThemeIconImageKey {
    pub(crate) fn new(icon_name: Arc<str>, icon_size_px: u32, scale_factor: f32) -> Self {
        Self {
            icon_name,
            icon_size_px: icon_size_px.clamp(1, 1024),
            scale_bits: stable_scale_bits(scale_factor),
            theme_name: Arc::from(UNKNOWN_THEME_NAME),
            color_scheme: IconColorScheme::Unknown,
            mode: IconPaintMode::Normal,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ThemeIconImageReadiness {
    ready: Arc<HashSet<ThemeIconImageKey>>,
    ready_paths: Arc<HashSet<PathBuf>>,
    order: VecDeque<ThemeIconImageKey>,
    path_order: VecDeque<PathBuf>,
    max_entries: usize,
}

impl Default for ThemeIconImageReadiness {
    fn default() -> Self {
        Self {
            ready: Arc::new(HashSet::new()),
            ready_paths: Arc::new(HashSet::new()),
            order: VecDeque::new(),
            path_order: VecDeque::new(),
            max_entries: DEFAULT_THEME_ICON_READINESS_LIMIT,
        }
    }
}

impl ThemeIconImageReadiness {
    pub(crate) fn snapshot(&self) -> ThemeIconImageReadinessSnapshot {
        ThemeIconImageReadinessSnapshot {
            ready: self.ready.clone(),
            ready_paths: self.ready_paths.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn mark_ready(&mut self, key: ThemeIconImageKey) -> bool {
        self.mark_ready_for_path(key, None)
    }

    pub(crate) fn mark_ready_path(&mut self, key: ThemeIconImageKey, path: Arc<Path>) -> bool {
        self.mark_ready_for_path(key, Some(path.as_ref()))
    }

    fn mark_ready_for_path(&mut self, key: ThemeIconImageKey, path: Option<&Path>) -> bool {
        let mut changed = false;
        if self.ready.contains(&key) {
            if let Some(path) = path {
                changed |= self.mark_path_ready(path);
            }
            return changed;
        }

        Arc::make_mut(&mut self.ready).insert(key.clone());
        self.order.push_back(key);
        changed = true;
        if let Some(path) = path {
            changed |= self.mark_path_ready(path);
        }
        while self.ready.len() > self.max_entries {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            Arc::make_mut(&mut self.ready).remove(&evicted);
        }
        changed
    }

    fn mark_path_ready(&mut self, path: &Path) -> bool {
        let path = path.to_path_buf();
        if self.ready_paths.contains(&path) {
            return false;
        }

        Arc::make_mut(&mut self.ready_paths).insert(path.clone());
        self.path_order.push_back(path);
        while self.ready_paths.len() > self.max_entries {
            let Some(evicted) = self.path_order.pop_front() else {
                break;
            };
            Arc::make_mut(&mut self.ready_paths).remove(&evicted);
        }
        true
    }

    #[cfg(test)]
    fn with_max_entries(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ThemeIconImageReadinessSnapshot {
    ready: Arc<HashSet<ThemeIconImageKey>>,
    ready_paths: Arc<HashSet<PathBuf>>,
}

impl ThemeIconImageReadinessSnapshot {
    pub(crate) fn is_ready(&self, key: &ThemeIconImageKey) -> bool {
        self.ready.contains(key)
    }

    pub(crate) fn is_path_ready(&self, path: &Path) -> bool {
        self.ready_paths.contains(path)
    }
}

pub(crate) fn theme_icon_image_key_for_snapshot(
    icon: &FileIconSnapshot,
    icon_size_px: u32,
    scale_factor: f32,
) -> Option<ThemeIconImageKey> {
    icon.path
        .as_ref()
        .map(|_| ThemeIconImageKey::new(icon.icon_name.clone(), icon_size_px, scale_factor))
}

pub(crate) fn theme_icon_image_size_px(width: f32, height: f32) -> u32 {
    width.min(height).round().clamp(1.0, 1024.0) as u32
}

fn stable_scale_bits(scale_factor: f32) -> u32 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor.to_bits()
    } else {
        1.0f32.to_bits()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ThemeIconImageStatus {
    Loaded,
    Pending,
    Failed,
    StalePath,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RetainedThemeIconImage<T> {
    pub(crate) key: ThemeIconImageKey,
    pub(crate) resolved_path: Option<PathBuf>,
    pub(crate) image: Option<T>,
    pub(crate) load_generation: u64,
    pub(crate) status: ThemeIconImageStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RetainedThemeIconImageLoad<T> {
    pub(crate) image: Option<T>,
    pub(crate) outcome: RetainedThemeIconImageLoadOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetainedThemeIconImageLoadOutcome {
    Loaded { first_ready: bool },
    Retained { status: ThemeIconImageStatus },
    Missing { status: ThemeIconImageStatus },
}

#[derive(Clone, Debug)]
pub(crate) struct RetainedThemeIconImageCache<T> {
    entries: HashMap<ThemeIconImageKey, RetainedThemeIconImage<T>>,
    images_by_path: HashMap<PathBuf, T>,
    load_generation: u64,
}

impl<T> Default for RetainedThemeIconImageCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            images_by_path: HashMap::new(),
            load_generation: 0,
        }
    }
}

impl<T: Clone> RetainedThemeIconImageCache<T> {
    pub(crate) fn record_loaded(
        &mut self,
        key: ThemeIconImageKey,
        resolved_path: Arc<Path>,
        image: T,
    ) -> RetainedThemeIconImageLoad<T> {
        self.load_generation += 1;
        let resolved_path_buf = resolved_path.as_ref().to_path_buf();
        let first_ready = self
            .entries
            .get(&key)
            .and_then(|entry| entry.image.as_ref())
            .is_none()
            && !self.images_by_path.contains_key(&resolved_path_buf);
        self.images_by_path
            .insert(resolved_path_buf.clone(), image.clone());
        let entry = RetainedThemeIconImage {
            key: key.clone(),
            resolved_path: Some(resolved_path_buf),
            image: Some(image.clone()),
            load_generation: self.load_generation,
            status: ThemeIconImageStatus::Loaded,
        };
        self.entries.insert(key, entry);
        RetainedThemeIconImageLoad {
            image: Some(image),
            outcome: RetainedThemeIconImageLoadOutcome::Loaded { first_ready },
        }
    }

    pub(crate) fn record_pending(
        &mut self,
        key: ThemeIconImageKey,
        resolved_path: Arc<Path>,
    ) -> RetainedThemeIconImageLoad<T> {
        self.record_unready(key, resolved_path, ThemeIconImageStatus::Pending)
    }

    pub(crate) fn record_failed(
        &mut self,
        key: ThemeIconImageKey,
        resolved_path: Arc<Path>,
    ) -> RetainedThemeIconImageLoad<T> {
        self.record_unready(key, resolved_path, ThemeIconImageStatus::Failed)
    }

    #[cfg(test)]
    fn get(&self, key: &ThemeIconImageKey) -> Option<&RetainedThemeIconImage<T>> {
        self.entries.get(key)
    }

    fn record_unready(
        &mut self,
        key: ThemeIconImageKey,
        resolved_path: Arc<Path>,
        requested_status: ThemeIconImageStatus,
    ) -> RetainedThemeIconImageLoad<T> {
        self.load_generation += 1;
        let resolved_path_buf = resolved_path.as_ref().to_path_buf();
        let path_image = self.images_by_path.get(&resolved_path_buf).cloned();
        let entry = self
            .entries
            .entry(key.clone())
            .or_insert_with(|| RetainedThemeIconImage {
                key,
                resolved_path: Some(resolved_path_buf.clone()),
                image: path_image.clone(),
                load_generation: self.load_generation,
                status: requested_status,
            });
        let status = if entry
            .resolved_path
            .as_deref()
            .is_some_and(|existing| existing != resolved_path.as_ref())
            && entry.image.is_some()
        {
            ThemeIconImageStatus::StalePath
        } else {
            requested_status
        };
        entry.resolved_path = Some(resolved_path_buf);
        entry.load_generation = self.load_generation;
        entry.status = status;

        if entry.image.is_none() {
            entry.image = path_image;
        }
        let image = entry.image.clone();
        let outcome = if image.is_some() {
            RetainedThemeIconImageLoadOutcome::Retained { status }
        } else {
            RetainedThemeIconImageLoadOutcome::Missing { status }
        };
        RetainedThemeIconImageLoad { image, outcome }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_keeps_icon_size_and_scale_in_identity() {
        let icon = icon_snapshot("text-x-generic", "/theme/48/text-x-generic.svg");

        let size_48 = theme_icon_image_key_for_snapshot(&icon, 48, 1.0).unwrap();
        let size_64 = theme_icon_image_key_for_snapshot(&icon, 64, 1.0).unwrap();
        let scaled = theme_icon_image_key_for_snapshot(&icon, 48, 2.0).unwrap();

        assert_eq!(size_48.icon_name.as_ref(), "text-x-generic");
        assert_ne!(size_48, size_64);
        assert_ne!(size_48, scaled);
    }

    #[test]
    fn missing_path_does_not_create_theme_image_key() {
        let mut icon = icon_snapshot("text-x-generic", "/theme/48/text-x-generic.svg");
        icon.path = None;

        assert!(theme_icon_image_key_for_snapshot(&icon, 48, 1.0).is_none());
    }

    #[test]
    fn cache_reuses_loaded_same_key_image_while_pending() {
        let key = ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0);
        let path: Arc<Path> = Arc::from(Path::new("/theme/48/text-x-generic.svg"));
        let mut cache = RetainedThemeIconImageCache::default();

        let loaded = cache.record_loaded(key.clone(), path.clone(), "image-48");
        let pending = cache.record_pending(key.clone(), path);

        assert_eq!(
            loaded.outcome,
            RetainedThemeIconImageLoadOutcome::Loaded { first_ready: true }
        );
        assert_eq!(
            pending.outcome,
            RetainedThemeIconImageLoadOutcome::Retained {
                status: ThemeIconImageStatus::Pending
            }
        );
        assert_eq!(pending.image, Some("image-48"));
        assert_eq!(
            cache.get(&key).unwrap().status,
            ThemeIconImageStatus::Pending
        );
    }

    #[test]
    fn cache_does_not_reuse_different_icon_size() {
        let key_48 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0);
        let key_64 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 64, 1.0);
        let path_48: Arc<Path> = Arc::from(Path::new("/theme/48/text-x-generic.svg"));
        let path_64: Arc<Path> = Arc::from(Path::new("/theme/64/text-x-generic.svg"));
        let mut cache = RetainedThemeIconImageCache::default();

        cache.record_loaded(key_48, path_48, "image-48");
        let pending_64 = cache.record_pending(key_64, path_64);

        assert_eq!(
            pending_64.outcome,
            RetainedThemeIconImageLoadOutcome::Missing {
                status: ThemeIconImageStatus::Pending
            }
        );
        assert_eq!(pending_64.image, None);
    }

    #[test]
    fn cache_reuses_loaded_same_path_image_for_new_size_key() {
        let key_48 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0);
        let key_64 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 64, 1.0);
        let path: Arc<Path> = Arc::from(Path::new("/theme/scalable/text-x-generic.svg"));
        let mut cache = RetainedThemeIconImageCache::default();

        cache.record_loaded(key_48, path.clone(), "image-scalable");
        let pending_64 = cache.record_pending(key_64.clone(), path.clone());
        let loaded_64 = cache.record_loaded(key_64, path, "image-scalable");

        assert_eq!(
            pending_64.outcome,
            RetainedThemeIconImageLoadOutcome::Retained {
                status: ThemeIconImageStatus::Pending
            }
        );
        assert_eq!(pending_64.image, Some("image-scalable"));
        assert_eq!(
            loaded_64.outcome,
            RetainedThemeIconImageLoadOutcome::Loaded { first_ready: false }
        );
    }

    #[test]
    fn cache_reuses_same_key_image_during_stale_path_refresh() {
        let key = ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0);
        let mut cache = RetainedThemeIconImageCache::default();

        cache.record_loaded(
            key.clone(),
            Arc::from(Path::new("/theme-old/48/text-x-generic.svg")),
            "old-image",
        );
        let pending = cache.record_pending(
            key,
            Arc::from(Path::new("/theme-new/48/text-x-generic.svg")),
        );

        assert_eq!(
            pending.outcome,
            RetainedThemeIconImageLoadOutcome::Retained {
                status: ThemeIconImageStatus::StalePath
            }
        );
        assert_eq!(pending.image, Some("old-image"));
    }

    #[test]
    fn readiness_snapshot_tracks_ready_keys_and_eviction() {
        let key_48 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0);
        let key_64 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 64, 1.0);
        let mut readiness = ThemeIconImageReadiness::with_max_entries(1);

        assert!(readiness.mark_ready(key_48.clone()));
        assert!(readiness.snapshot().is_ready(&key_48));
        assert!(!readiness.mark_ready(key_48.clone()));

        assert!(readiness.mark_ready(key_64.clone()));
        let snapshot = readiness.snapshot();
        assert!(!snapshot.is_ready(&key_48));
        assert!(snapshot.is_ready(&key_64));
    }

    #[test]
    fn readiness_snapshot_tracks_ready_resource_paths() {
        let key_48 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0);
        let key_64 = ThemeIconImageKey::new(Arc::from("image-png"), 48, 1.0);
        let path_48: Arc<Path> = Arc::from(Path::new("/theme/48/text-x-generic.svg"));
        let path_64: Arc<Path> = Arc::from(Path::new("/theme/48/image-png.svg"));
        let mut readiness = ThemeIconImageReadiness::with_max_entries(1);

        assert!(readiness.mark_ready_path(key_48.clone(), path_48.clone()));
        assert!(readiness.snapshot().is_path_ready(path_48.as_ref()));
        assert!(!readiness.mark_ready_path(key_48, path_48.clone()));

        assert!(readiness.mark_ready_path(key_64, path_64.clone()));
        let snapshot = readiness.snapshot();
        assert!(!snapshot.is_path_ready(path_48.as_ref()));
        assert!(snapshot.is_path_ready(path_64.as_ref()));
    }

    fn icon_snapshot(icon_name: &str, path: &str) -> FileIconSnapshot {
        FileIconSnapshot {
            icon_name: Arc::from(icon_name),
            path: Some(Arc::from(Path::new(path))),
            fallback_marker: Arc::from("TXT"),
            fallback_fg: 0xffffff,
            fallback_bg: 0x2563eb,
        }
    }
}
