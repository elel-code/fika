use crate::AppWindow;
use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::events::AsyncEvent;
use crate::app::geometry::{ITEM_VIEW_OVERSCAN_COLUMNS, ItemViewLayouter};
use crate::app::pane::PaneState;
use crate::app::state::AppState;
use crate::app::thumbnail_pipeline::{
    ThumbnailScheduleEntry, ThumbnailScheduleRow, thumbnail_schedule_batch_for_pane,
};
use crate::app::zoom::icon_size_for_zoom_level;
use crate::fs::thumbnails;
use std::cell::RefCell;
use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub(crate) const ICON_SIZE_UPDATE_INTERVAL: Duration = Duration::from_millis(300);
pub(crate) const RESOLVE_ALL_ITEMS_LIMIT: usize = 500;
pub(crate) const READ_AHEAD_PAGES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThumbnailScheduleScope {
    VisibleOnly,
    VisibleAndReadAhead,
}

pub(crate) fn thumbnail_size_px(ui: &AppWindow) -> u32 {
    thumbnail_size_px_for_zoom_level(ui.get_icon_zoom_level())
}

pub(crate) fn thumbnail_size_px_for_zoom_level(zoom_level: i32) -> u32 {
    icon_size_for_zoom_level(zoom_level)
}

pub(crate) fn schedule_visible_thumbnail_roles_for_slot(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
) {
    schedule_thumbnail_roles_for_slot_with_scope(
        ui,
        state,
        bridge,
        slot,
        ThumbnailScheduleScope::VisibleOnly,
    );
}

fn schedule_thumbnail_roles_for_slot_with_scope(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    scope: ThumbnailScheduleScope,
) {
    let size_px = thumbnail_size_px(ui);
    let (pane_id, virtual_start_index, visible_range, maximum_visible_items, entries) = {
        let state_ref = state.borrow();
        let Some(pane) = state_ref.panes.pane_for_slot(slot) else {
            return;
        };
        let entries = pane
            .view
            .virtual_entry_tokens
            .iter()
            .map(ThumbnailScheduleEntry::from_row_token)
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return;
        }
        let visible_range = current_pane_visible_range(pane);
        (
            pane.id,
            pane.view.virtual_start_index,
            visible_range.clone(),
            maximum_visible_items_for_range(&visible_range),
            entries,
        )
    };

    schedule_thumbnail_roles_for_entries_with_scope(
        state,
        bridge,
        pane_id,
        &entries,
        virtual_start_index,
        visible_range,
        maximum_visible_items,
        size_px,
        scope,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn schedule_thumbnail_roles_for_entries<T>(
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pane_id: u64,
    entries: &[T],
    virtual_start_index: usize,
    visible_range: Range<usize>,
    maximum_visible_items: usize,
    size_px: u32,
) where
    T: ThumbnailScheduleRow,
{
    schedule_thumbnail_roles_for_entries_with_scope(
        state,
        bridge,
        pane_id,
        entries,
        virtual_start_index,
        visible_range,
        maximum_visible_items,
        size_px,
        ThumbnailScheduleScope::VisibleAndReadAhead,
    );
}

#[allow(clippy::too_many_arguments)]
fn schedule_thumbnail_roles_for_entries_with_scope<T>(
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    pane_id: u64,
    entries: &[T],
    virtual_start_index: usize,
    visible_range: Range<usize>,
    maximum_visible_items: usize,
    size_px: u32,
    scope: ThumbnailScheduleScope,
) where
    T: ThumbnailScheduleRow,
{
    let ordered_indexes = indexes_to_resolve_for_slice_with_scope(
        entries,
        virtual_start_index,
        visible_range,
        maximum_visible_items,
        scope,
    );
    if ordered_indexes.is_empty() {
        return;
    }

    let (generation, paths) = {
        let mut state = state.borrow_mut();
        let Some(pane) = state.panes.pane_by_id(pane_id) else {
            return;
        };
        let generation = pane.thumbnail_generation.current();
        let paths = thumbnail_schedule_batch_for_pane(
            &mut state,
            pane_id,
            ordered_indexes
                .into_iter()
                .filter_map(|index| entries.get(index)),
            size_px,
        );

        (generation, paths)
    };

    if paths.is_empty() {
        return;
    }

    spawn_thumbnail_preview_job(bridge, pane_id, generation, paths, size_px);
}

pub(crate) fn current_pane_visible_range(pane: &PaneState) -> Range<usize> {
    pane.view
        .virtual_view
        .layout
        .as_ref()
        .map(|layout| {
            layout
                .virtual_plan(pane.view.viewport_x, ITEM_VIEW_OVERSCAN_COLUMNS)
                .visible_range
        })
        .unwrap_or(0..0)
}

#[cfg(test)]
pub(crate) fn indexes_to_resolve_for_slice<T: ThumbnailScheduleRow>(
    entries: &[T],
    virtual_start_index: usize,
    visible_range: Range<usize>,
    maximum_visible_items: usize,
) -> Vec<usize> {
    indexes_to_resolve_for_slice_with_scope(
        entries,
        virtual_start_index,
        visible_range,
        maximum_visible_items,
        ThumbnailScheduleScope::VisibleAndReadAhead,
    )
}

fn indexes_to_resolve_for_slice_with_scope<T: ThumbnailScheduleRow>(
    entries: &[T],
    virtual_start_index: usize,
    visible_range: Range<usize>,
    maximum_visible_items: usize,
    scope: ThumbnailScheduleScope,
) -> Vec<usize> {
    if entries.is_empty() {
        return Vec::new();
    }

    let entry_count = virtual_start_index.saturating_add(entries.len());
    let visible_first = visible_range.start.min(entry_count);
    let visible_last_exclusive = visible_range.end.min(entry_count).max(visible_first);
    let mut indexes =
        visible_indexes_to_resolve_for_slice(entries, virtual_start_index, visible_range.clone());
    match scope {
        ThumbnailScheduleScope::VisibleOnly => indexes,
        ThumbnailScheduleScope::VisibleAndReadAhead => {
            indexes.extend(
                indexes_to_resolve(entry_count, visible_range, maximum_visible_items)
                    .into_iter()
                    .filter(|index| *index < visible_first || *index >= visible_last_exclusive)
                    .filter_map(|index| index.checked_sub(virtual_start_index))
                    .filter(|&index| index < entries.len()),
            );
            indexes
        }
    }
}

pub(crate) fn visible_indexes_to_resolve_for_slice<T: ThumbnailScheduleRow>(
    entries: &[T],
    virtual_start_index: usize,
    visible_range: Range<usize>,
) -> Vec<usize> {
    if entries.is_empty() {
        return Vec::new();
    }

    let entry_count = virtual_start_index.saturating_add(entries.len());
    let visible_first = visible_range.start.min(entry_count);
    let visible_last_exclusive = visible_range.end.min(entry_count).max(visible_first);
    let mut indexes = Vec::new();
    let mut visible_dirs = Vec::new();

    for index in visible_first..visible_last_exclusive {
        let Some(local_index) = index.checked_sub(virtual_start_index) else {
            continue;
        };
        let Some(entry) = entries.get(local_index) else {
            continue;
        };
        match entry.is_dir() {
            true => visible_dirs.push(local_index),
            false => indexes.push(local_index),
        };
    }
    indexes.extend(visible_dirs);
    indexes
}

pub(crate) fn indexes_to_resolve(
    entry_count: usize,
    visible_range: Range<usize>,
    maximum_visible_items: usize,
) -> Vec<usize> {
    if entry_count == 0 {
        return Vec::new();
    }

    let maximum_visible_items = maximum_visible_items.max(1);
    let first_visible_index = visible_range.start.min(entry_count.saturating_sub(1));
    let last_visible_index = visible_range
        .end
        .saturating_sub(1)
        .min(entry_count.saturating_sub(1))
        .max(first_visible_index);
    let mut result = Vec::with_capacity(entry_count.min(
        last_visible_index.saturating_sub(first_visible_index)
            + 1
            + RESOLVE_ALL_ITEMS_LIMIT
            + 2 * maximum_visible_items,
    ));

    for index in first_visible_index..=last_visible_index {
        result.push(index);
    }

    let read_ahead_items =
        (READ_AHEAD_PAGES * maximum_visible_items).min(RESOLVE_ALL_ITEMS_LIMIT / 2);
    let end_extended_visible_range =
        (last_visible_index + read_ahead_items).min(entry_count.saturating_sub(1));
    for index in last_visible_index.saturating_add(1)..=end_extended_visible_range {
        result.push(index);
    }

    let begin_extended_visible_range = first_visible_index.saturating_sub(read_ahead_items);
    for index in (begin_extended_visible_range..first_visible_index).rev() {
        result.push(index);
    }

    let begin_last_page =
        (entry_count.saturating_sub(maximum_visible_items)).max(end_extended_visible_range + 1);
    for index in begin_last_page..entry_count {
        result.push(index);
    }

    let end_first_page = begin_extended_visible_range.min(maximum_visible_items);
    for index in 0..end_first_page {
        result.push(index);
    }

    let mut remaining_items = RESOLVE_ALL_ITEMS_LIMIT.saturating_sub(result.len());
    for index in end_extended_visible_range.saturating_add(1)..begin_last_page {
        if remaining_items == 0 {
            break;
        }
        result.push(index);
        remaining_items -= 1;
    }

    for index in (end_first_page..begin_extended_visible_range).rev() {
        if remaining_items == 0 {
            break;
        }
        result.push(index);
        remaining_items -= 1;
    }

    result
}

fn maximum_visible_items_for_range(visible_range: &Range<usize>) -> usize {
    visible_range.end.saturating_sub(visible_range.start).max(1)
}

fn spawn_thumbnail_preview_job(
    bridge: &AsyncBridge,
    pane_id: u64,
    generation: u64,
    paths: Vec<PathBuf>,
    size_px: u32,
) {
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let fallback_paths = paths.clone();
        let loads = match tokio::task::spawn_blocking(move || {
            paths
                .into_iter()
                .map(|path| thumbnails::load_thumbnail(path, size_px))
                .collect::<Vec<_>>()
        })
        .await
        {
            Ok(loads) => loads,
            Err(err) => {
                let message = format!("thumbnail task failed: {err}");
                fallback_paths
                    .into_iter()
                    .map(|path| thumbnails::ThumbnailLoad {
                        key: thumbnails::fallback_key(&path, size_px),
                        path,
                        cache_paths: None,
                        data: Err(io::Error::other(message.clone())),
                    })
                    .collect()
            }
        };
        for load in loads {
            send_async_event(
                async_tx.clone(),
                notify_ui.clone(),
                AsyncEvent::ThumbnailLoaded {
                    pane_id,
                    generation,
                    load,
                },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::thumbnail_pipeline::ThumbnailScheduleRow;

    #[derive(Clone)]
    struct TestRow {
        path: String,
        is_dir: bool,
    }

    impl ThumbnailScheduleRow for TestRow {
        fn path(&self) -> &str {
            &self.path
        }

        fn is_dir(&self) -> bool {
            self.is_dir
        }

        fn thumbnail_state(&self) -> i32 {
            0
        }

        fn media_token(&self) -> i32 {
            0
        }
    }

    #[test]
    fn indexes_to_resolve_matches_dolphin_visible_then_readahead_order() {
        let indexes = indexes_to_resolve(40, 12..16, 4);

        assert_eq!(&indexes[..4], &[12, 13, 14, 15]);
        assert_eq!(&indexes[4..24], &(16..36).collect::<Vec<_>>());
        assert_eq!(&indexes[24..36], &(0..12).rev().collect::<Vec<_>>());
        assert_eq!(&indexes[36..40], &(36..40).collect::<Vec<_>>());
    }

    #[test]
    fn indexes_to_resolve_for_slice_keeps_dolphin_global_order_inside_virtual_slice() {
        let entries = (8..20)
            .map(|index| TestRow {
                path: format!("/tmp/{index}.png"),
                is_dir: false,
            })
            .collect::<Vec<_>>();
        let indexes = indexes_to_resolve_for_slice(&entries, 8, 12..16, 4);

        assert_eq!(indexes, vec![4, 5, 6, 7, 8, 9, 10, 11, 3, 2, 1, 0]);
    }

    #[test]
    fn visible_indexes_to_resolve_for_slice_scans_once_with_files_before_dirs() {
        let entries = (8..20)
            .map(|index| TestRow {
                path: format!("/tmp/{index}"),
                is_dir: matches!(index, 10 | 12 | 15),
            })
            .collect::<Vec<_>>();

        let indexes = visible_indexes_to_resolve_for_slice(&entries, 8, 10..16);

        assert_eq!(indexes, vec![3, 5, 6, 2, 4, 7]);
    }

    #[test]
    fn thumbnail_size_px_tracks_zoom_levels() {
        assert_eq!(thumbnail_size_px_for_zoom_level(0), 16);
        assert_eq!(thumbnail_size_px_for_zoom_level(1), 22);
        assert_eq!(thumbnail_size_px_for_zoom_level(2), 32);
        assert_eq!(thumbnail_size_px_for_zoom_level(3), 48);
        assert_eq!(thumbnail_size_px_for_zoom_level(4), 64);
        assert_eq!(thumbnail_size_px_for_zoom_level(5), 80);
        assert_eq!(thumbnail_size_px_for_zoom_level(16), 256);
    }
}
