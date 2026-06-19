use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::FileIconSnapshot;

const UNKNOWN_THEME_NAME: &str = "__fika_unknown_icon_theme__";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum IconColorScheme {
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum IconPaintMode {
    Normal,
}

// Pixmap identity, not file-role identity. Dolphin stores model data as
// iconName, then KStandardItemListWidget::pixmapForIcon() keys QPixmapCache by
// icon name, requested size, DPR, and mode.
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EvictedThemeIconImage<T> {
    pub(crate) resolved_path: Option<PathBuf>,
    pub(crate) image: T,
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
    load_generation: u64,
}

impl<T> Default for RetainedThemeIconImageCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            load_generation: 0,
        }
    }
}

impl<T: Clone> RetainedThemeIconImageCache<T> {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

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
            .is_none();
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

    pub(crate) fn image_for_key(&mut self, key: &ThemeIconImageKey) -> Option<T> {
        let image = self
            .entries
            .get(key)
            .and_then(|entry| entry.image.clone())?;
        self.touch_key(key);
        Some(image)
    }

    pub(crate) fn prune_to_budget<F>(
        &mut self,
        max_bytes: usize,
        mut image_cost: F,
    ) -> Vec<EvictedThemeIconImage<T>>
    where
        F: FnMut(&T) -> usize,
    {
        let max_bytes = max_bytes.max(1);
        let mut total_bytes = self
            .entries
            .values()
            .filter_map(|entry| entry.image.as_ref())
            .map(&mut image_cost)
            .sum::<usize>();
        let mut evicted_images = Vec::new();
        while total_bytes > max_bytes {
            let Some(evicted_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.load_generation)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            let Some(evicted) = self.entries.remove(&evicted_key) else {
                continue;
            };
            if let Some(path) = evicted.resolved_path {
                if let Some(image) = evicted.image {
                    total_bytes = total_bytes.saturating_sub(image_cost(&image));
                    evicted_images.push(EvictedThemeIconImage {
                        resolved_path: Some(path),
                        image,
                    });
                }
            } else if let Some(image) = evicted.image {
                total_bytes = total_bytes.saturating_sub(image_cost(&image));
                evicted_images.push(EvictedThemeIconImage {
                    resolved_path: None,
                    image,
                });
            }
        }
        evicted_images
    }

    fn record_unready(
        &mut self,
        key: ThemeIconImageKey,
        resolved_path: Arc<Path>,
        requested_status: ThemeIconImageStatus,
    ) -> RetainedThemeIconImageLoad<T> {
        self.load_generation += 1;
        let resolved_path_buf = resolved_path.as_ref().to_path_buf();
        let entry = self
            .entries
            .entry(key.clone())
            .or_insert_with(|| RetainedThemeIconImage {
                key,
                resolved_path: Some(resolved_path_buf.clone()),
                image: None,
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

        let image = entry.image.clone();
        let outcome = if image.is_some() {
            RetainedThemeIconImageLoadOutcome::Retained { status }
        } else {
            RetainedThemeIconImageLoadOutcome::Missing { status }
        };
        RetainedThemeIconImageLoad { image, outcome }
    }

    fn touch_key(&mut self, key: &ThemeIconImageKey) {
        self.load_generation += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.load_generation = self.load_generation;
        }
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
    fn cache_prunes_by_image_budget_and_releases_least_recent_key_images() {
        let key_48 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0);
        let key_64 = ThemeIconImageKey::new(Arc::from("text-x-generic"), 64, 1.0);
        let key_png = ThemeIconImageKey::new(Arc::from("image-png"), 48, 1.0);
        let scalable_path: Arc<Path> = Arc::from(Path::new("/theme/scalable/text-x-generic.svg"));
        let png_path: Arc<Path> = Arc::from(Path::new("/theme/48/image-png.svg"));
        let mut cache = RetainedThemeIconImageCache::default();

        cache.record_loaded(key_48.clone(), scalable_path.clone(), "image-48");
        cache.record_loaded(key_64.clone(), scalable_path.clone(), "image-64");
        cache.record_loaded(key_png.clone(), png_path.clone(), "image-png");
        assert_eq!(cache.len(), 3);

        let evicted = cache.prune_to_budget(2, |_| 2);
        assert_eq!(
            evicted,
            vec![
                EvictedThemeIconImage {
                    resolved_path: Some(scalable_path.as_ref().to_path_buf()),
                    image: "image-48"
                },
                EvictedThemeIconImage {
                    resolved_path: Some(scalable_path.as_ref().to_path_buf()),
                    image: "image-64"
                }
            ]
        );
        assert_eq!(cache.len(), 1);
        assert!(cache.image_for_key(&key_48).is_none());
        assert!(cache.image_for_key(&key_64).is_none());
        assert!(cache.image_for_key(&key_png).is_some());
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
