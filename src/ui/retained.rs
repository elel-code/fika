use std::collections::{HashMap, VecDeque};
use std::fs;
use std::hash::Hash;
use std::path::Path;
use std::sync::Arc;

use gpui::{App, Entity, RenderImage, Resource, RetainAllImageCache, Window};

use crate::FikaApp;
use crate::ui::icons::{
    EvictedThemeIconImage, FileIconSnapshot, RetainedThemeIconImageLoadOutcome, ThemeIconImageKey,
    theme_icon_image_key_for_snapshot,
};

mod work_order;

pub(crate) use work_order::{
    dolphin_read_ahead_indexes, visible_work_range, visit_dolphin_visible_work_files_first,
    visit_visible_work_items_by_index,
};

const THEME_ICON_PIXMAP_CACHE_LIMIT_KB: usize = 10 * 1024;
const RETAINED_THUMBNAIL_CACHE_LIMIT_KB: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RetainedSlotStats {
    pub(crate) inserted: usize,
    pub(crate) content_changed: usize,
    pub(crate) geometry_changed: usize,
    pub(crate) visual_changed: usize,
    pub(crate) unchanged: usize,
    pub(crate) removed: usize,
    pub(crate) entries: usize,
}

impl RetainedSlotStats {
    pub(crate) fn has_activity(self) -> bool {
        self.inserted > 0
            || self.content_changed > 0
            || self.geometry_changed > 0
            || self.visual_changed > 0
            || self.unchanged > 0
            || self.removed > 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TextShapeCacheStats {
    pub(crate) hits: usize,
    pub(crate) misses: usize,
    pub(crate) evicted: usize,
    pub(crate) entries: usize,
}

impl TextShapeCacheStats {
    pub(crate) fn has_activity(self) -> bool {
        self.hits > 0 || self.misses > 0 || self.evicted > 0
    }
}

pub(crate) struct RetainedShapeCache<K, V> {
    entries: HashMap<K, V>,
    stats: TextShapeCacheStats,
    max_entries: usize,
}

impl<K, V> RetainedShapeCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            stats: TextShapeCacheStats::default(),
            max_entries,
        }
    }

    pub(crate) fn get_or_insert_with<F>(&mut self, key: &K, shape: F) -> V
    where
        F: FnOnce(&K) -> V,
    {
        if let Some(value) = self.entries.get(key) {
            self.stats.hits += 1;
            return value.clone();
        }

        self.stats.misses += 1;
        if self.entries.len() >= self.max_entries.max(1) {
            self.stats.evicted += self.entries.len();
            self.entries.clear();
        }

        let value = shape(key);
        self.entries.insert(key.clone(), value.clone());
        value
    }

    pub(crate) fn take_stats(&mut self) -> TextShapeCacheStats {
        let mut stats = std::mem::take(&mut self.stats);
        stats.entries = self.entries.len();
        stats
    }
}

pub(crate) fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(crate) struct RetainedImageLayerState {
    image_cache: Entity<RetainAllImageCache>,
    retained_thumbnails: RetainedThumbnailCache<Arc<RenderImage>>,
}

struct RetainedThumbnailCache<T> {
    entries: HashMap<Arc<Path>, RetainedThumbnailImage<T>>,
    order: VecDeque<Arc<Path>>,
    bytes: usize,
    limit_bytes: usize,
}

struct RetainedThumbnailImage<T> {
    image: T,
    cost_bytes: usize,
}

impl<T> RetainedThumbnailCache<T> {
    fn new(limit_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            limit_bytes,
        }
    }

    fn contains_key(&self, source_path: &Arc<Path>) -> bool {
        self.entries.contains_key(source_path)
    }

    fn insert(
        &mut self,
        source_path: Arc<Path>,
        image: T,
        cost_bytes: usize,
    ) -> Vec<(Arc<Path>, T)> {
        if let Some(previous) = self.entries.remove(&source_path) {
            self.bytes = self.bytes.saturating_sub(previous.cost_bytes);
        }
        self.bytes = self.bytes.saturating_add(cost_bytes);
        self.entries.insert(
            source_path.clone(),
            RetainedThumbnailImage { image, cost_bytes },
        );
        self.touch(source_path);
        self.prune()
    }

    fn touch(&mut self, source_path: Arc<Path>) {
        self.order
            .retain(|path| path.as_ref() != source_path.as_ref());
        self.order.push_back(source_path);
    }

    fn prune(&mut self) -> Vec<(Arc<Path>, T)> {
        let mut evicted = Vec::new();
        while self.bytes > self.limit_bytes && self.entries.len() > 1 {
            let Some(evicted_path) = self.order.pop_front() else {
                break;
            };
            let Some(evicted_image) = self.entries.remove(&evicted_path) else {
                continue;
            };
            self.bytes = self.bytes.saturating_sub(evicted_image.cost_bytes);
            evicted.push((evicted_path, evicted_image.image));
        }
        evicted
    }
}

impl<T: Clone> RetainedThumbnailCache<T> {
    fn get_cloned_and_touch(&mut self, source_path: &Arc<Path>) -> Option<T> {
        let image = self
            .entries
            .get(source_path)
            .map(|retained| retained.image.clone());
        if image.is_some() {
            self.touch(source_path.clone());
        }
        image
    }
}

pub(crate) struct RetainedImageLoad {
    pub(crate) image: Option<Arc<RenderImage>>,
    pub(crate) outcome: RetainedImageLoadOutcome,
    pub(crate) ready: Option<RetainedImageReady>,
}

#[derive(Clone, Debug)]
pub(crate) enum RetainedImageRequest {
    Thumbnail {
        source_path: Arc<Path>,
    },
    ThemeIcon {
        source_path: Arc<Path>,
        key: ThemeIconImageKey,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetainedImageRequestKind {
    Thumbnail,
    ThemeIcon,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RetainedImageReady {
    ThemeIcon {
        source_path: Arc<Path>,
        key: ThemeIconImageKey,
    },
}

impl RetainedImageReady {
    pub(crate) fn theme_icon(source_path: Arc<Path>, key: ThemeIconImageKey) -> Self {
        Self::ThemeIcon { source_path, key }
    }
}

impl RetainedImageRequest {
    pub(crate) fn thumbnail(source_path: Arc<Path>) -> Self {
        Self::Thumbnail { source_path }
    }

    pub(crate) fn theme_icon(source_path: Arc<Path>, key: ThemeIconImageKey) -> Self {
        Self::ThemeIcon { source_path, key }
    }

    pub(crate) fn theme_icon_for_parts(
        source_path: Option<Arc<Path>>,
        icon_name: Arc<str>,
        icon_size_px: u32,
        scale_factor: f32,
    ) -> Option<Self> {
        let source_path = source_path?;
        Some(Self::theme_icon(
            source_path,
            ThemeIconImageKey::new(icon_name, icon_size_px, scale_factor),
        ))
    }

    pub(crate) fn theme_icon_for_snapshot(
        icon: &FileIconSnapshot,
        icon_size_px: u32,
        scale_factor: f32,
    ) -> Option<Self> {
        let source_path = icon.path.clone()?;
        let key = theme_icon_image_key_for_snapshot(icon, icon_size_px, scale_factor)?;
        Some(Self::theme_icon(source_path, key))
    }

    pub(crate) fn thumbnail_or_theme_icon_for_snapshot(
        thumbnail_path: Option<Arc<Path>>,
        icon: &FileIconSnapshot,
        icon_size_px: u32,
        scale_factor: f32,
    ) -> Option<Self> {
        if let Some(source_path) = thumbnail_path {
            return Some(Self::thumbnail(source_path));
        }

        Self::theme_icon_for_snapshot(icon, icon_size_px, scale_factor)
    }

    pub(crate) fn source_path(&self) -> &Arc<Path> {
        match self {
            Self::Thumbnail { source_path } | Self::ThemeIcon { source_path, .. } => source_path,
        }
    }

    pub(crate) fn kind(&self) -> RetainedImageRequestKind {
        match self {
            Self::Thumbnail { .. } => RetainedImageRequestKind::Thumbnail,
            Self::ThemeIcon { .. } => RetainedImageRequestKind::ThemeIcon,
        }
    }

    #[cfg(test)]
    pub(crate) fn theme_icon_key(&self) -> Option<&ThemeIconImageKey> {
        match self {
            Self::ThemeIcon { key, .. } => Some(key),
            Self::Thumbnail { .. } => None,
        }
    }

    pub(crate) fn ready_state(&self) -> Option<RetainedImageReady> {
        match self {
            Self::ThemeIcon { source_path, key } => Some(RetainedImageReady::theme_icon(
                source_path.clone(),
                key.clone(),
            )),
            Self::Thumbnail { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetainedImageLoadOutcome {
    CacheReady { first_ready: bool },
    Retained,
    Missing,
}

impl RetainedImageLayerState {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            image_cache: RetainAllImageCache::new(cx),
            retained_thumbnails: RetainedThumbnailCache::new(retained_thumbnail_cache_limit_bytes()),
        }
    }

    fn load_thumbnail_or_retained_with_outcome(
        &mut self,
        source_path: Arc<Path>,
        window: &mut Window,
        cx: &mut App,
    ) -> RetainedImageLoad {
        let resource = Resource::Path(source_path.clone());
        let load_result = self
            .image_cache
            .update(cx, |cache, cx| cache.load(&resource, window, cx));
        match load_result {
            Some(Ok(image)) => {
                let first_ready = !self.retained_thumbnails.contains_key(&source_path);
                self.retain_thumbnail_image(source_path, image.clone(), window, cx);
                RetainedImageLoad {
                    image: Some(image),
                    outcome: RetainedImageLoadOutcome::CacheReady { first_ready },
                    ready: None,
                }
            }
            _ => {
                let image = self.retained_thumbnails.get_cloned_and_touch(&source_path);
                let outcome = if image.is_some() {
                    RetainedImageLoadOutcome::Retained
                } else {
                    RetainedImageLoadOutcome::Missing
                };
                RetainedImageLoad {
                    image,
                    outcome,
                    ready: None,
                }
            }
        }
    }

    fn retain_thumbnail_image(
        &mut self,
        source_path: Arc<Path>,
        image: Arc<RenderImage>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let cost_bytes = render_image_cache_cost_bytes(&image);
        for (evicted_path, evicted_image) in
            self.retained_thumbnails
                .insert(source_path, image, cost_bytes)
        {
            let resource = Resource::Path(evicted_path);
            self.image_cache
                .update(cx, |cache, cx| cache.remove(&resource, window, cx));
            cx.drop_image(evicted_image, Some(window));
        }
    }

    pub(crate) fn load_request_or_retained_with_outcome(
        &mut self,
        request: RetainedImageRequest,
        app: &gpui::WeakEntity<FikaApp>,
        window: &mut Window,
        cx: &mut App,
    ) -> RetainedImageLoad {
        let ready = request.ready_state();
        match request {
            RetainedImageRequest::Thumbnail { source_path } => self
                .load_thumbnail_or_retained_with_outcome(source_path, window, cx)
                .with_ready_state(ready),
            RetainedImageRequest::ThemeIcon { source_path, key } => self
                .load_theme_icon_or_retained_with_outcome(source_path, key, app, window, cx)
                .with_ready_state(ready),
        }
    }

    fn load_theme_icon_or_retained_with_outcome(
        &mut self,
        source_path: Arc<Path>,
        key: ThemeIconImageKey,
        app: &gpui::WeakEntity<FikaApp>,
        window: &mut Window,
        cx: &mut App,
    ) -> RetainedImageLoad {
        if let Ok(Some(retained)) = app.update(cx, |this, cx| {
            this.load_retained_or_sync_svg_theme_icon(
                source_path.clone(),
                key.clone(),
                cx,
                Some(window),
            )
        }) {
            self.prune_retained_theme_icon_images(app, window, cx);
            return retained;
        }

        let resource = Resource::Path(source_path.clone());
        let load_result = self
            .image_cache
            .update(cx, |cache, cx| cache.load(&resource, window, cx));
        let load = app
            .update(cx, |this, _cx| {
                this.record_theme_icon_resource_load_result(source_path, key, load_result)
            })
            .unwrap_or(RetainedImageLoad {
                image: None,
                outcome: RetainedImageLoadOutcome::Missing,
                ready: None,
            });
        self.prune_retained_theme_icon_images(app, window, cx);
        load
    }

    fn prune_retained_theme_icon_images(
        &mut self,
        app: &gpui::WeakEntity<FikaApp>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let evicted = app
            .update(cx, |this, cx| this.prune_retained_theme_icon_images(cx))
            .unwrap_or_default();
        for evicted in evicted {
            if let Some(path) = evicted.resolved_path {
                let resource = Resource::Path(Arc::<Path>::from(path.into_boxed_path()));
                self.image_cache
                    .update(cx, |cache, cx| cache.remove(&resource, window, cx));
            }
            cx.drop_image(evicted.image, Some(window));
        }
    }
}

impl Default for RetainedImageLoad {
    fn default() -> Self {
        Self {
            image: None,
            outcome: RetainedImageLoadOutcome::Missing,
            ready: None,
        }
    }
}

impl RetainedImageLoad {
    fn with_ready_state(mut self, ready: Option<RetainedImageReady>) -> Self {
        self.ready = self.image.is_some().then_some(ready).flatten();
        self
    }
}

fn render_image_cache_cost_bytes(image: &Arc<RenderImage>) -> usize {
    (0..image.frame_count())
        .filter_map(|frame_index| image.as_bytes(frame_index).map(|bytes| bytes.len()))
        .sum::<usize>()
        .max(1)
}

fn theme_icon_pixmap_cache_limit_bytes() -> usize {
    THEME_ICON_PIXMAP_CACHE_LIMIT_KB * 1024
}

fn retained_thumbnail_cache_limit_bytes() -> usize {
    RETAINED_THUMBNAIL_CACHE_LIMIT_KB * 1024
}

fn is_svg_icon_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

fn load_svg_theme_icon_sync(path: &Path, cx: &mut App) -> Option<Arc<RenderImage>> {
    let bytes = fs::read(path).ok()?;
    cx.svg_renderer().render_single_frame(&bytes, 1.0).ok()
}

impl FikaApp {
    pub(crate) fn mark_retained_image_ready(&mut self, ready: RetainedImageReady) -> bool {
        match ready {
            RetainedImageReady::ThemeIcon { source_path, key } => {
                self.theme_icon_readiness.mark_ready_path(key, source_path)
            }
        }
    }

    pub(crate) fn load_retained_or_sync_svg_theme_icon(
        &mut self,
        source_path: Arc<Path>,
        key: ThemeIconImageKey,
        cx: &mut App,
        window: Option<&mut Window>,
    ) -> Option<RetainedImageLoad> {
        if let Some(image) = self.theme_icon_images.image_for_key(&key) {
            return Some(RetainedImageLoad {
                image: Some(image),
                outcome: RetainedImageLoadOutcome::Retained,
                ready: None,
            });
        }

        if !is_svg_icon_path(source_path.as_ref()) {
            return None;
        }

        if let Some(image) = self
            .theme_icon_images
            .image_for_source_path(source_path.as_ref())
        {
            let retained =
                self.theme_icon_images
                    .record_loaded_from_retained_source(key, source_path, image);
            return Some(RetainedImageLoad {
                image: retained.image,
                outcome: retained_theme_icon_load_outcome(retained.outcome),
                ready: None,
            });
        }

        let image = load_svg_theme_icon_sync(source_path.as_ref(), cx)?;
        let retained = self
            .theme_icon_images
            .record_loaded(key, source_path, image);
        if window.is_none() {
            let _ = self.prune_retained_theme_icon_images(cx);
        }
        Some(RetainedImageLoad {
            image: retained.image,
            outcome: retained_theme_icon_load_outcome(retained.outcome),
            ready: None,
        })
    }

    fn record_theme_icon_resource_load_result(
        &mut self,
        source_path: Arc<Path>,
        key: ThemeIconImageKey,
        load_result: Option<Result<Arc<RenderImage>, gpui::ImageCacheError>>,
    ) -> RetainedImageLoad {
        let retained = match load_result {
            Some(Ok(image)) => self
                .theme_icon_images
                .record_loaded(key, source_path, image),
            Some(Err(_)) => self.theme_icon_images.record_failed(key, source_path),
            None => self.theme_icon_images.record_pending(key, source_path),
        };
        RetainedImageLoad {
            image: retained.image,
            outcome: retained_theme_icon_load_outcome(retained.outcome),
            ready: None,
        }
    }

    fn prune_retained_theme_icon_images(
        &mut self,
        _cx: &mut App,
    ) -> Vec<EvictedThemeIconImage<Arc<RenderImage>>> {
        self.theme_icon_images.prune_to_budget(
            theme_icon_pixmap_cache_limit_bytes(),
            render_image_cache_cost_bytes,
        )
    }
}

fn retained_theme_icon_load_outcome(
    outcome: RetainedThemeIconImageLoadOutcome,
) -> RetainedImageLoadOutcome {
    match outcome {
        RetainedThemeIconImageLoadOutcome::Loaded { first_ready } => {
            RetainedImageLoadOutcome::CacheReady { first_ready }
        }
        RetainedThemeIconImageLoadOutcome::Retained { .. } => RetainedImageLoadOutcome::Retained,
        RetainedThemeIconImageLoadOutcome::Missing { .. } => RetainedImageLoadOutcome::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumbnail_path(path: &str) -> Arc<Path> {
        Arc::from(Path::new(path))
    }

    #[test]
    fn env_flag_truthy_values_are_explicit() {
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
    fn retained_slot_stats_activity_tracks_all_change_kinds() {
        assert!(!RetainedSlotStats::default().has_activity());
        assert!(
            RetainedSlotStats {
                unchanged: 1,
                ..Default::default()
            }
            .has_activity()
        );
        assert!(
            RetainedSlotStats {
                removed: 1,
                ..Default::default()
            }
            .has_activity()
        );
    }

    #[test]
    fn text_shape_cache_stats_activity_ignores_entry_count() {
        assert!(
            !TextShapeCacheStats {
                entries: 4,
                ..Default::default()
            }
            .has_activity()
        );
        assert!(
            TextShapeCacheStats {
                misses: 1,
                entries: 4,
                ..Default::default()
            }
            .has_activity()
        );
    }

    #[test]
    fn retained_shape_cache_tracks_hits_misses_and_capacity_eviction() {
        let mut cache = RetainedShapeCache::new(2);

        assert_eq!(cache.get_or_insert_with(&1, |key| key * 10), 10);
        assert_eq!(cache.get_or_insert_with(&1, |key| key * 100), 10);
        assert_eq!(cache.get_or_insert_with(&2, |key| key * 10), 20);

        assert_eq!(
            cache.take_stats(),
            TextShapeCacheStats {
                hits: 1,
                misses: 2,
                evicted: 0,
                entries: 2,
            }
        );

        assert_eq!(cache.get_or_insert_with(&3, |key| key * 10), 30);
        assert_eq!(
            cache.take_stats(),
            TextShapeCacheStats {
                hits: 0,
                misses: 1,
                evicted: 2,
                entries: 1,
            }
        );
    }

    #[test]
    fn retained_image_load_ready_state_requires_loaded_image() {
        let ready = RetainedImageReady::theme_icon(
            thumbnail_path("/theme/48/text-x-generic.svg"),
            ThemeIconImageKey::new(Arc::from("text-x-generic"), 48, 1.0),
        );

        let missing = RetainedImageLoad::default().with_ready_state(Some(ready.clone()));
        assert_eq!(missing.ready, None);

        let loaded = RetainedImageLoad {
            image: Some(Arc::new(RenderImage::new(Vec::new()))),
            outcome: RetainedImageLoadOutcome::Retained,
            ready: None,
        }
        .with_ready_state(Some(ready.clone()));
        assert_eq!(loaded.ready, Some(ready));
    }

    #[test]
    fn retained_thumbnail_cache_evicts_least_recently_used_entry() {
        let mut cache = RetainedThumbnailCache::new(10);
        let a = thumbnail_path("/thumb/a.png");
        let b = thumbnail_path("/thumb/b.png");
        let c = thumbnail_path("/thumb/c.png");

        assert!(cache.insert(a.clone(), 1, 4).is_empty());
        assert!(cache.insert(b.clone(), 2, 4).is_empty());

        let evicted = cache.insert(c.clone(), 3, 4);

        assert_eq!(evicted, vec![(a.clone(), 1)]);
        assert!(!cache.contains_key(&a));
        assert!(cache.contains_key(&b));
        assert!(cache.contains_key(&c));
        assert_eq!(cache.bytes, 8);
    }

    #[test]
    fn retained_thumbnail_cache_get_refreshes_lru_order() {
        let mut cache = RetainedThumbnailCache::new(10);
        let a = thumbnail_path("/thumb/a.png");
        let b = thumbnail_path("/thumb/b.png");
        let c = thumbnail_path("/thumb/c.png");

        cache.insert(a.clone(), 1, 4);
        cache.insert(b.clone(), 2, 4);
        assert_eq!(cache.get_cloned_and_touch(&a), Some(1));

        let evicted = cache.insert(c.clone(), 3, 4);

        assert_eq!(evicted, vec![(b.clone(), 2)]);
        assert!(cache.contains_key(&a));
        assert!(!cache.contains_key(&b));
        assert!(cache.contains_key(&c));
    }

    #[test]
    fn retained_thumbnail_cache_keeps_single_oversized_entry() {
        let mut cache = RetainedThumbnailCache::new(10);
        let oversized = thumbnail_path("/thumb/oversized.png");

        let evicted = cache.insert(oversized.clone(), 1, 64);

        assert!(evicted.is_empty());
        assert!(cache.contains_key(&oversized));
        assert_eq!(cache.bytes, 64);
    }

    #[test]
    fn retained_thumbnail_cache_reinsert_updates_cost_and_recency() {
        let mut cache = RetainedThumbnailCache::new(10);
        let a = thumbnail_path("/thumb/a.png");
        let b = thumbnail_path("/thumb/b.png");
        let c = thumbnail_path("/thumb/c.png");

        cache.insert(a.clone(), 1, 8);
        cache.insert(b.clone(), 2, 1);
        assert!(cache.insert(a.clone(), 10, 1).is_empty());
        assert_eq!(cache.bytes, 2);

        let evicted = cache.insert(c.clone(), 3, 9);

        assert_eq!(evicted, vec![(b.clone(), 2)]);
        assert_eq!(cache.get_cloned_and_touch(&a), Some(10));
        assert!(cache.contains_key(&c));
        assert_eq!(cache.bytes, 10);
    }
}
