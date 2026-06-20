use std::collections::{HashMap, VecDeque};
use std::fs;
use std::hash::Hash;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    App, Entity, RenderImage, Resource, RetainAllImageCache, SMOOTH_SVG_SCALE_FACTOR, Window,
};

use crate::FikaApp;
use crate::ui::icons::{
    EvictedThemeIconImage, FileIconSnapshot, IconPaintMode, RetainedThemeIconImageLoadOutcome,
    ThemeIconImageKey, theme_icon_image_key_for_snapshot_with_mode,
};

mod work_order;

pub(crate) use work_order::{
    dolphin_read_ahead_indexes, dolphin_visible_work_indexes,
    visit_dolphin_visible_work_files_first, visit_visible_work_items_by_index,
};

// Dolphin keeps ordinary MIME/theme icon pixmaps in QPixmapCache from
// KStandardItemListWidget::pixmapForIcon(), keyed by icon name, size, DPR, and
// mode. This is the GPUI custom-paint equivalent for decoded theme images.
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
    pub(crate) compute_us: u128,
    pub(crate) entries: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RetainedThemeIconPruneStats {
    pub(crate) evicted: usize,
    pub(crate) entries: usize,
    pub(crate) bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RetainedThemeIconCacheRefreshStats {
    pub(crate) requested: usize,
    pub(crate) retained: usize,
    pub(crate) loaded: usize,
    pub(crate) decoded: usize,
    pub(crate) missing: usize,
    pub(crate) non_svg: usize,
    pub(crate) evicted: usize,
    pub(crate) cache_entries: usize,
    pub(crate) cache_bytes: usize,
    pub(crate) elapsed_us: u128,
}

impl RetainedThemeIconCacheRefreshStats {
    pub(crate) fn has_activity(self) -> bool {
        self.requested > 0
            || self.retained > 0
            || self.loaded > 0
            || self.missing > 0
            || self.non_svg > 0
    }

    pub(crate) fn record_load(&mut self, load: Option<RetainedImageLoad>) {
        match load.map(|load| load.outcome) {
            Some(RetainedImageLoadOutcome::CacheReady { first_ready }) => {
                self.loaded += 1;
                if first_ready {
                    self.decoded += 1;
                }
            }
            Some(RetainedImageLoadOutcome::Retained) => {
                self.retained += 1;
            }
            Some(RetainedImageLoadOutcome::Missing) => {
                self.missing += 1;
            }
            None => {
                self.non_svg += 1;
            }
        }
    }

    pub(crate) fn record_prune(&mut self, prune: RetainedThemeIconPruneStats) {
        self.evicted = prune.evicted;
        self.cache_entries = prune.entries;
        self.cache_bytes = prune.bytes;
    }
}

impl TextShapeCacheStats {
    pub(crate) fn has_activity(self) -> bool {
        self.hits > 0 || self.misses > 0 || self.evicted > 0 || self.compute_us > 0
    }
}

pub(crate) struct RetainedShapeCache<K, V> {
    entries: HashMap<K, RetainedShapeCacheEntry<V>>,
    stats: TextShapeCacheStats,
    max_entries: usize,
    retention_frame_window: u64,
    retention_epoch: u64,
    retention_active: bool,
}

struct RetainedShapeCacheEntry<V> {
    value: V,
    last_used_epoch: u64,
}

impl<K, V> RetainedShapeCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    pub(crate) fn new(max_entries: usize) -> Self {
        Self::new_with_retention_frame_window(max_entries, 1)
    }

    pub(crate) fn new_with_retention_frame_window(
        max_entries: usize,
        retention_frame_window: u64,
    ) -> Self {
        Self {
            entries: HashMap::new(),
            stats: TextShapeCacheStats::default(),
            max_entries,
            retention_frame_window: retention_frame_window.max(1),
            retention_epoch: 0,
            retention_active: false,
        }
    }

    pub(crate) fn get_or_insert_with<F>(&mut self, key: &K, shape: F) -> V
    where
        F: FnOnce(&K) -> V,
    {
        if let Some(value) = self.entries.get_mut(key) {
            self.stats.hits += 1;
            value.last_used_epoch = self.retention_epoch;
            return value.value.clone();
        }

        self.stats.misses += 1;
        if self.entries.len() >= self.max_entries.max(1) {
            self.stats.evicted += self.entries.len();
            self.entries.clear();
        }

        let started = Instant::now();
        let value = shape(key);
        self.stats.compute_us += started.elapsed().as_micros();
        self.insert_entry(key.clone(), value.clone());
        value
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.entries.get_mut(key) {
            self.stats.hits += 1;
            value.last_used_epoch = self.retention_epoch;
            Some(value.value.clone())
        } else {
            self.stats.misses += 1;
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn peek(&self, key: &K) -> Option<V> {
        self.entries.get(key).map(|entry| entry.value.clone())
    }

    pub(crate) fn peek_and_touch(&mut self, key: &K) -> Option<V> {
        self.entries.get_mut(key).map(|entry| {
            entry.last_used_epoch = self.retention_epoch;
            entry.value.clone()
        })
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        if !self.entries.contains_key(&key) && self.entries.len() >= self.max_entries.max(1) {
            self.stats.evicted += self.entries.len();
            self.entries.clear();
        }
        self.insert_entry(key, value);
    }

    pub(crate) fn begin_retention_frame(&mut self) {
        self.retention_epoch = self.retention_epoch.wrapping_add(1).max(1);
        self.retention_active = true;
    }

    pub(crate) fn finish_retention_frame(&mut self) {
        if !self.retention_active {
            return;
        }

        let epoch = self.retention_epoch;
        let min_epoch = epoch
            .saturating_sub(self.retention_frame_window.saturating_sub(1))
            .max(1);
        let before_retain = self.entries.len();
        self.entries.retain(|_, entry| {
            entry.last_used_epoch >= min_epoch && entry.last_used_epoch <= epoch
        });
        self.stats.evicted += before_retain.saturating_sub(self.entries.len());
        self.retention_active = false;
    }

    pub(crate) fn take_stats(&mut self) -> TextShapeCacheStats {
        let mut stats = std::mem::take(&mut self.stats);
        stats.entries = self.entries.len();
        stats
    }

    fn insert_entry(&mut self, key: K, value: V) {
        self.entries.insert(
            key,
            RetainedShapeCacheEntry {
                value,
                last_used_epoch: self.retention_epoch,
            },
        );
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

impl RetainedImageRequest {
    pub(crate) fn thumbnail(source_path: Arc<Path>) -> Self {
        Self::Thumbnail { source_path }
    }

    pub(crate) fn theme_icon(source_path: Arc<Path>, key: ThemeIconImageKey) -> Self {
        Self::ThemeIcon { source_path, key }
    }

    pub(crate) fn theme_icon_for_parts_with_mode(
        source_path: Option<Arc<Path>>,
        icon_name: Arc<str>,
        icon_size_px: u32,
        scale_factor: f32,
        mode: IconPaintMode,
    ) -> Option<Self> {
        let source_path = source_path?;
        Some(Self::theme_icon(
            source_path,
            ThemeIconImageKey::new_with_mode(icon_name, icon_size_px, scale_factor, mode),
        ))
    }

    pub(crate) fn theme_icon_for_snapshot(
        icon: &FileIconSnapshot,
        icon_size_px: u32,
        scale_factor: f32,
    ) -> Option<Self> {
        Self::theme_icon_for_snapshot_with_mode(
            icon,
            icon_size_px,
            scale_factor,
            IconPaintMode::Normal,
        )
    }

    pub(crate) fn theme_icon_for_snapshot_with_mode(
        icon: &FileIconSnapshot,
        icon_size_px: u32,
        scale_factor: f32,
        mode: IconPaintMode,
    ) -> Option<Self> {
        let source_path = icon.path.clone()?;
        let key =
            theme_icon_image_key_for_snapshot_with_mode(icon, icon_size_px, scale_factor, mode)?;
        Some(Self::theme_icon(source_path, key))
    }

    pub(crate) fn thumbnail_or_theme_icon_for_snapshot_with_mode(
        thumbnail_path: Option<Arc<Path>>,
        icon: &FileIconSnapshot,
        icon_size_px: u32,
        scale_factor: f32,
        mode: IconPaintMode,
    ) -> Option<Self> {
        if let Some(source_path) = thumbnail_path {
            return Some(Self::thumbnail(source_path));
        }

        Self::theme_icon_for_snapshot_with_mode(icon, icon_size_px, scale_factor, mode)
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

    pub(crate) fn into_theme_icon_parts(self) -> Option<(Arc<Path>, ThemeIconImageKey)> {
        match self {
            Self::ThemeIcon { source_path, key } => Some((source_path, key)),
            Self::Thumbnail { .. } => None,
        }
    }

    pub(crate) fn theme_icon_key(&self) -> Option<&ThemeIconImageKey> {
        match self {
            Self::ThemeIcon { key, .. } => Some(key),
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
                }
            }
            _ => {
                let image = self.retained_thumbnails.get_cloned_and_touch(&source_path);
                let outcome = if image.is_some() {
                    RetainedImageLoadOutcome::Retained
                } else {
                    RetainedImageLoadOutcome::Missing
                };
                RetainedImageLoad { image, outcome }
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
        match request {
            RetainedImageRequest::Thumbnail { source_path } => {
                self.load_thumbnail_or_retained_with_outcome(source_path, window, cx)
            }
            RetainedImageRequest::ThemeIcon { source_path, key } => {
                self.load_theme_icon_or_retained_with_outcome(source_path, key, app, window, cx)
            }
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
        if let Ok(Some(retained)) =
            app.update(cx, |this, _cx| this.retained_theme_icon_image_for_key(&key))
        {
            return retained;
        }

        if is_svg_icon_path(source_path.as_ref()) {
            return RetainedImageLoad::default();
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
        }
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

fn load_svg_theme_icon_sync(
    path: &Path,
    key: &ThemeIconImageKey,
    cx: &mut App,
) -> Option<Arc<RenderImage>> {
    let bytes = fs::read(path).ok()?;
    let scale_factor = svg_theme_icon_render_scale(&bytes, key);
    cx.svg_renderer()
        .render_single_frame(&bytes, scale_factor)
        .ok()
}

fn svg_theme_icon_render_scale(bytes: &[u8], key: &ThemeIconImageKey) -> f32 {
    let source_width = svg_intrinsic_width(bytes)
        .filter(|width| width.is_finite() && *width > 0.0)
        .unwrap_or(key.icon_size_px as f32);
    let target_device_px = ((key.icon_size_px as f32) * key.scale_factor())
        .round()
        .clamp(1.0, 4096.0);
    (target_device_px / source_width / SMOOTH_SVG_SCALE_FACTOR).clamp(0.01, 128.0)
}

fn svg_intrinsic_width(bytes: &[u8]) -> Option<f32> {
    let text = std::str::from_utf8(bytes).ok()?;
    let start = text.find("<svg")?;
    let rest = &text[start..];
    let end = rest.find('>')?;
    let tag = &rest[..end];
    svg_attribute(tag, "width")
        .and_then(parse_svg_dimension)
        .or_else(|| svg_view_box_width(tag))
}

fn svg_view_box_width(tag: &str) -> Option<f32> {
    let view_box = svg_attribute(tag, "viewBox").or_else(|| svg_attribute(tag, "viewbox"))?;
    view_box
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .nth(2)
        .and_then(parse_svg_dimension)
}

fn svg_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut offset = 0;
    while let Some(found) = tag[offset..].find(name) {
        let start = offset + found;
        let before = tag[..start].chars().next_back();
        if before.is_some_and(|ch| !(ch.is_ascii_whitespace() || ch == '<')) {
            offset = start + name.len();
            continue;
        }
        let mut rest = tag[start + name.len()..].trim_start();
        if !rest.starts_with('=') {
            offset = start + name.len();
            continue;
        }
        rest = rest[1..].trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_start = quote.len_utf8();
        let value_end = rest[value_start..].find(quote)?;
        return Some(&rest[value_start..value_start + value_end]);
    }
    None
}

fn parse_svg_dimension(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.ends_with('%') {
        return None;
    }
    let end = value
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-'))
        .map(|(index, ch)| index + ch.len_utf8())
        .last()?;
    value[..end].parse::<f32>().ok()
}

impl FikaApp {
    pub(crate) fn retained_theme_icon_image_for_key(
        &mut self,
        key: &ThemeIconImageKey,
    ) -> Option<RetainedImageLoad> {
        self.theme_icon_images
            .image_for_key(key)
            .map(|image| RetainedImageLoad {
                image: Some(image),
                outcome: RetainedImageLoadOutcome::Retained,
            })
    }

    pub(crate) fn refresh_retained_theme_icon_cache(
        &mut self,
        source_path: Arc<Path>,
        key: ThemeIconImageKey,
        cx: &mut App,
    ) -> Option<RetainedImageLoad> {
        if let Some(retained) = self.retained_theme_icon_image_for_key(&key) {
            return Some(retained);
        }

        if !is_svg_icon_path(source_path.as_ref()) {
            return None;
        }

        let retained = if let Some(image) = load_svg_theme_icon_sync(source_path.as_ref(), &key, cx)
        {
            self.theme_icon_images
                .record_loaded(key, source_path, image)
        } else {
            self.theme_icon_images.record_failed(key, source_path)
        };
        Some(RetainedImageLoad {
            image: retained.image,
            outcome: retained_theme_icon_load_outcome(retained.outcome),
        })
    }

    pub(crate) fn refresh_visible_retained_theme_icon_requests(
        &mut self,
        requests: impl IntoIterator<Item = RetainedImageRequest>,
        cx: &mut App,
        window: &mut Window,
    ) -> RetainedThemeIconCacheRefreshStats {
        self.refresh_retained_theme_icon_requests_with_decode_budget(
            requests,
            crate::THEME_ICON_VISIBLE_DECODE_BUDGET,
            cx,
            window,
        )
    }

    pub(crate) fn refresh_retained_theme_icon_requests_retained_only(
        &mut self,
        requests: impl IntoIterator<Item = RetainedImageRequest>,
        cx: &mut App,
        window: &mut Window,
    ) -> RetainedThemeIconCacheRefreshStats {
        self.refresh_retained_theme_icon_requests_with_decode_budget(requests, 0, cx, window)
    }

    pub(crate) fn refresh_retained_theme_icon_requests_with_decode_budget(
        &mut self,
        requests: impl IntoIterator<Item = RetainedImageRequest>,
        decode_budget: usize,
        cx: &mut App,
        window: &mut Window,
    ) -> RetainedThemeIconCacheRefreshStats {
        let started = Instant::now();
        let mut stats = RetainedThemeIconCacheRefreshStats::default();
        for request in requests {
            let Some((source_path, key)) = request.into_theme_icon_parts() else {
                continue;
            };
            if let Some(retained) = self.retained_theme_icon_image_for_key(&key) {
                stats.requested += 1;
                stats.record_load(Some(retained));
                continue;
            }
            if stats.decoded >= decode_budget && is_svg_icon_path(source_path.as_ref()) {
                continue;
            }

            stats.requested += 1;
            stats.record_load(self.refresh_retained_theme_icon_cache(source_path, key, cx));
        }
        if stats.requested > 0 {
            let prune_stats = self.prune_retained_theme_icon_cache(cx, window);
            stats.record_prune(prune_stats);
            stats.elapsed_us = started.elapsed().as_micros();
        }
        stats
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

    pub(crate) fn retained_theme_icon_cache_stats(&self) -> RetainedThemeIconPruneStats {
        RetainedThemeIconPruneStats {
            entries: self.theme_icon_images.len(),
            bytes: self
                .theme_icon_images
                .loaded_cost(render_image_cache_cost_bytes),
            evicted: 0,
        }
    }

    pub(crate) fn prune_retained_theme_icon_cache(
        &mut self,
        cx: &mut App,
        window: &mut Window,
    ) -> RetainedThemeIconPruneStats {
        let evicted = self.prune_retained_theme_icon_images(cx);
        let evicted_count = evicted.len();
        for evicted in evicted {
            cx.drop_image(evicted.image, Some(window));
        }
        let mut stats = self.retained_theme_icon_cache_stats();
        stats.evicted = evicted_count;
        stats
    }

    pub(crate) fn clear_retained_theme_icon_cache(
        &mut self,
        cx: &mut App,
        window: &mut Window,
    ) -> RetainedThemeIconPruneStats {
        let evicted = self.theme_icon_images.drain_loaded();
        let evicted_count = evicted.len();
        for evicted in evicted {
            cx.drop_image(evicted.image, Some(window));
        }
        RetainedThemeIconPruneStats {
            evicted: evicted_count,
            entries: self.theme_icon_images.len(),
            bytes: self
                .theme_icon_images
                .loaded_cost(render_image_cache_cost_bytes),
        }
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
    fn svg_intrinsic_width_prefers_width_then_view_box() {
        assert_eq!(
            svg_intrinsic_width(br#"<svg width="22" height="22" viewBox="0 0 48 48"></svg>"#),
            Some(22.0)
        );
        assert_eq!(
            svg_intrinsic_width(br#"<svg viewBox="0 0 48 48"></svg>"#),
            Some(48.0)
        );
        assert_eq!(
            svg_intrinsic_width(br#"<svg width="24px" height="24px"></svg>"#),
            Some(24.0)
        );
    }

    #[test]
    fn svg_theme_icon_render_scale_targets_requested_device_pixels() {
        let key = ThemeIconImageKey::new(Arc::from("folder"), 22, 1.0);
        let scale = svg_theme_icon_render_scale(br#"<svg width="44" height="44"></svg>"#, &key);
        assert!((scale - 0.25).abs() < f32::EPSILON);

        let key = ThemeIconImageKey::new(Arc::from("folder"), 22, 2.0);
        let scale = svg_theme_icon_render_scale(br#"<svg width="22" height="22"></svg>"#, &key);
        assert!((scale - 1.0).abs() < f32::EPSILON);
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
        assert!(
            TextShapeCacheStats {
                compute_us: 1,
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

        let stats = cache.take_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.evicted, 0);
        assert_eq!(stats.entries, 2);

        assert_eq!(cache.get_or_insert_with(&3, |key| key * 10), 30);
        let stats = cache.take_stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.evicted, 2);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn retained_shape_cache_peek_does_not_record_cache_activity() {
        let mut cache = RetainedShapeCache::new(2);

        assert_eq!(cache.get_or_insert_with(&1, |key| key * 10), 10);
        let _ = cache.take_stats();

        assert_eq!(cache.peek(&1), Some(10));
        assert_eq!(cache.peek(&2), None);

        let stats = cache.take_stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evicted, 0);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn retained_shape_cache_retention_frame_evicts_stale_entries() {
        let mut cache = RetainedShapeCache::new(4);

        assert_eq!(cache.get_or_insert_with(&1, |key| key * 10), 10);
        assert_eq!(cache.get_or_insert_with(&2, |key| key * 10), 20);
        let _ = cache.take_stats();

        cache.begin_retention_frame();
        assert_eq!(cache.get_or_insert_with(&2, |key| key * 100), 20);
        assert_eq!(cache.get_or_insert_with(&3, |key| key * 10), 30);
        cache.finish_retention_frame();

        assert_eq!(cache.peek(&1), None);
        assert_eq!(cache.peek(&2), Some(20));
        assert_eq!(cache.peek(&3), Some(30));
        let stats = cache.take_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.evicted, 1);
        assert_eq!(stats.entries, 2);
    }

    #[test]
    fn retained_shape_cache_can_keep_recent_retention_frames() {
        let mut cache = RetainedShapeCache::new_with_retention_frame_window(8, 2);

        cache.begin_retention_frame();
        assert_eq!(cache.get_or_insert_with(&1, |key| key * 10), 10);
        cache.finish_retention_frame();
        let _ = cache.take_stats();

        cache.begin_retention_frame();
        assert_eq!(cache.get_or_insert_with(&2, |key| key * 10), 20);
        cache.finish_retention_frame();

        assert_eq!(cache.peek(&1), Some(10));
        assert_eq!(cache.peek(&2), Some(20));
        let stats = cache.take_stats();
        assert_eq!(stats.evicted, 0);
        assert_eq!(stats.entries, 2);

        cache.begin_retention_frame();
        assert_eq!(cache.get_or_insert_with(&2, |key| key * 100), 20);
        assert_eq!(cache.get_or_insert_with(&3, |key| key * 10), 30);
        cache.finish_retention_frame();

        assert_eq!(cache.peek(&1), None);
        assert_eq!(cache.peek(&2), Some(20));
        assert_eq!(cache.peek(&3), Some(30));
        let stats = cache.take_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.evicted, 1);
        assert_eq!(stats.entries, 2);
    }

    #[test]
    fn retained_shape_cache_peek_and_touch_keeps_entry_without_stats() {
        let mut cache = RetainedShapeCache::new(4);

        assert_eq!(cache.get_or_insert_with(&1, |key| key * 10), 10);
        assert_eq!(cache.get_or_insert_with(&2, |key| key * 10), 20);
        let _ = cache.take_stats();

        cache.begin_retention_frame();
        assert_eq!(cache.peek_and_touch(&1), Some(10));
        cache.finish_retention_frame();

        assert_eq!(cache.peek(&1), Some(10));
        assert_eq!(cache.peek(&2), None);
        let stats = cache.take_stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evicted, 1);
        assert_eq!(stats.entries, 1);
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
