use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fika_core::{ItemId, ItemLayout};
use gpui::SharedString;

use crate::ui::icons::FileIconSnapshot;

use super::{FileGridIconRequest, RawVisibleItemSnapshot};

#[derive(Clone, Debug)]
pub(crate) struct VisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) visible: bool,
    pub(crate) item_id: ItemId,
    pub(crate) layout: ItemLayout,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) display_name: SharedString,
    pub(crate) thumbnail_path: Option<Arc<Path>>,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) fallback_marker: SharedString,
    pub(crate) icon_name_lines: Arc<[SharedString]>,
    pub(crate) drag_path: Arc<Path>,
    pub(crate) selected: bool,
    pub(crate) selection_count: usize,
    pub(crate) drop_target: bool,
    pub(crate) draft_name: Option<String>,
    pub(crate) draft_caret: Option<usize>,
    pub(crate) draft_selection: Option<(usize, usize)>,
    pub(crate) draft_error: Option<String>,
    pub(crate) draft_warning: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VisibleItemSnapshotCacheKey {
    path: PathBuf,
    is_dir: bool,
    name: Arc<str>,
    thumbnail_path: Option<PathBuf>,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    icon_size_px: u16,
    text_width_bits: u32,
    text_height_bits: u32,
}

#[derive(Clone, Debug)]
pub(super) struct VisibleItemSnapshotCacheEntry {
    key: VisibleItemSnapshotCacheKey,
    visible_epoch: u64,
    pub(super) is_dir: bool,
    pub(super) name: Arc<str>,
    pub(super) display_name: SharedString,
    pub(super) thumbnail_path: Option<Arc<Path>>,
    pub(super) icon: FileIconSnapshot,
    pub(super) fallback_marker: SharedString,
    pub(super) icon_name_lines: Arc<[SharedString]>,
    pub(super) drag_path: Arc<Path>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VisibleItemSnapshotCache {
    entries: HashMap<ItemId, VisibleItemSnapshotCacheEntry>,
    visible_epoch: u64,
}

impl VisibleItemSnapshotCache {
    pub(super) fn begin_visible_update(&mut self) {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
    }

    pub(super) fn retain_current_visible(&mut self) {
        let visible_epoch = self.visible_epoch;
        self.entries
            .retain(|_, entry| entry.visible_epoch == visible_epoch);
    }

    pub(super) fn content_for_raw_item<F>(
        &mut self,
        item: &RawVisibleItemSnapshot,
        cache_text_lines: bool,
        resolve_uncached: bool,
        icon_for_item: &mut F,
    ) -> Option<VisibleItemSnapshotCacheEntry>
    where
        F: for<'a> FnMut(FileGridIconRequest<'a>) -> FileIconSnapshot,
    {
        let key = visible_item_snapshot_cache_key(item, cache_text_lines);
        if let Some(entry) = self.entries.get_mut(&item.item_id)
            && entry.key == key
        {
            entry.visible_epoch = self.visible_epoch;
            return Some(entry.clone());
        }

        if !resolve_uncached {
            return None;
        }

        let icon = icon_for_item(FileGridIconRequest {
            path: &item.path,
            is_dir: item.is_dir,
            mime_type: item.mime_type.clone(),
            mime_magic_checked: item.mime_magic_checked,
            icon_size: item.layout.icon_rect.width,
        });
        let icon_name_lines = if cache_text_lines {
            super::super::layout::icon_name_display_lines(
                &item.name,
                icon_name_layout_width(item.layout.text_rect.width),
                icon_name_max_lines(item.layout.text_rect.height),
            )
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>()
            .into()
        } else {
            Vec::<SharedString>::new().into()
        };
        let entry = VisibleItemSnapshotCacheEntry {
            key,
            visible_epoch: self.visible_epoch,
            is_dir: item.is_dir,
            name: item.name.clone(),
            display_name: SharedString::from(item.name.as_ref()),
            thumbnail_path: item
                .thumbnail_path
                .as_ref()
                .map(|path| Arc::from(path.as_path())),
            fallback_marker: SharedString::from(icon.fallback_marker.as_ref()),
            icon,
            icon_name_lines,
            drag_path: Arc::from(item.path.as_path()),
        };
        self.entries.insert(item.item_id, entry.clone());
        Some(entry)
    }
}

fn visible_item_snapshot_cache_key(
    item: &RawVisibleItemSnapshot,
    cache_text_lines: bool,
) -> VisibleItemSnapshotCacheKey {
    VisibleItemSnapshotCacheKey {
        path: item.path.clone(),
        is_dir: item.is_dir,
        name: item.name.clone(),
        thumbnail_path: item.thumbnail_path.clone(),
        mime_type: item.mime_type.clone(),
        mime_magic_checked: item.mime_magic_checked,
        icon_size_px: item.layout.icon_rect.width.round().clamp(16.0, 256.0) as u16,
        text_width_bits: cache_text_lines
            .then(|| icon_name_layout_width(item.layout.text_rect.width).to_bits())
            .unwrap_or_default(),
        text_height_bits: cache_text_lines
            .then(|| item.layout.text_rect.height.to_bits())
            .unwrap_or_default(),
    }
}

const ICON_NAME_HORIZONTAL_SAFE_INSET: f32 = 6.0;

pub(super) fn icon_name_layout_width(text_rect_width: f32) -> f32 {
    (text_rect_width - ICON_NAME_HORIZONTAL_SAFE_INSET * 2.0).max(1.0)
}

pub(super) fn icon_name_max_lines(text_rect_height: f32) -> usize {
    (text_rect_height / super::super::ITEM_NAME_LINE_HEIGHT)
        .round()
        .max(1.0) as usize
}
