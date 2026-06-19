use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use gpui::{App, Entity, RenderImage, Resource, RetainAllImageCache, Window};

use crate::FikaApp;
use crate::ui::icons::{
    EvictedThemeIconImage, RetainedThemeIconImageLoadOutcome, ThemeIconImageKey,
};

const THEME_ICON_PIXMAP_CACHE_LIMIT_KB: usize = 10 * 1024;

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

pub(crate) fn env_flag_is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(crate) struct RetainedImageLayerState {
    image_cache: Entity<RetainAllImageCache>,
    retained_thumbnails: HashMap<Arc<Path>, Arc<RenderImage>>,
}

pub(crate) struct RetainedImageLoad {
    pub(crate) image: Option<Arc<RenderImage>>,
    pub(crate) outcome: RetainedImageLoadOutcome,
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
            retained_thumbnails: HashMap::new(),
        }
    }

    pub(crate) fn load_thumbnail_or_retained_with_outcome(
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
                self.retained_thumbnails.insert(source_path, image.clone());
                RetainedImageLoad {
                    image: Some(image),
                    outcome: RetainedImageLoadOutcome::CacheReady { first_ready },
                }
            }
            _ => {
                let image = self.retained_thumbnails.get(&source_path).cloned();
                let outcome = if image.is_some() {
                    RetainedImageLoadOutcome::Retained
                } else {
                    RetainedImageLoadOutcome::Missing
                };
                RetainedImageLoad { image, outcome }
            }
        }
    }

    pub(crate) fn load_theme_icon_or_retained(
        &mut self,
        source_path: Arc<Path>,
        key: ThemeIconImageKey,
        app: &gpui::WeakEntity<FikaApp>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<RenderImage>> {
        self.load_theme_icon_or_retained_with_outcome(source_path, key, app, window, cx)
            .image
    }

    pub(crate) fn load_theme_icon_or_retained_with_outcome(
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
        })
    }

    pub(crate) fn record_theme_icon_resource_load_result(
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

    pub(crate) fn prune_retained_theme_icon_images(
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
}
