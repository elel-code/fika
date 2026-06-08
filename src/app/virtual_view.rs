use crate::app::geometry::{
    CompactItemViewLayout, ITEM_VIEW_OVERSCAN_COLUMNS, ItemViewLayoutEngine, ItemViewLayouter,
    MainItemViewLayout, VirtualItemViewPlan,
};
use crate::app::item_view_model::{
    ItemViewModelEntry, ItemViewModelEntryArc, item_view_entry_matches_filters,
    item_view_filters_are_identity, item_view_model_entry_with_group,
};
use crate::app::pane::{PaneEntryModel, VirtualViewCache};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VirtualViewSnapshotUpdate {
    pub(crate) entry_count: usize,
    pub(crate) layout: Arc<ItemViewLayoutEngine>,
    pub(crate) viewport_x: f32,
    pub(crate) viewport_clamped: bool,
    pub(crate) range: Range<usize>,
    pub(crate) visible_range: Range<usize>,
    pub(crate) start_column: usize,
    pub(crate) entries: PaneEntryModel,
    pub(crate) rebuild_model: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualViewSnapshotInput {
    pub(crate) layout: MainItemViewLayout,
    pub(crate) requested_viewport_x: f32,
    pub(crate) range_hint: Option<Range<usize>>,
    pub(crate) thumbnail_size_px: u32,
    pub(crate) schedule_thumbnails: bool,
    pub(crate) force_rebuild_model: bool,
    pub(crate) visible_count_override: Option<usize>,
    pub(crate) cache: VirtualViewCache,
    pub(crate) entries: PaneEntryModel,
    pub(crate) visible_entry_indices: Option<Arc<[usize]>>,
    pub(crate) visible_entries_have_locations: bool,
    pub(crate) visible_location_groups: Option<Arc<[String]>>,
    pub(crate) query: String,
    pub(crate) kind_filter: i32,
    pub(crate) modified_filter: i32,
    pub(crate) size_filter: i32,
    pub(crate) chooser_patterns: Vec<String>,
}

pub(crate) fn prepare_virtual_view_snapshot_update(
    input: VirtualViewSnapshotInput,
) -> VirtualViewSnapshotUpdate {
    let visible_count = input
        .visible_count_override
        .unwrap_or_else(|| snapshot_visible_entry_count(&input));
    let item_view_layout = cached_snapshot_layout(&input, visible_count).unwrap_or_else(|| {
        Arc::new(ItemViewLayoutEngine::from(
            snapshot_compact_item_view_layout(&input, visible_count),
        ))
    });
    let compact_item_view = item_view_layout.as_compact();
    let plan =
        compact_item_view.virtual_plan(input.requested_viewport_x, ITEM_VIEW_OVERSCAN_COLUMNS);
    let range = compact_item_view
        .expand_virtual_range_to_hint(plan.range.clone(), input.range_hint.as_ref());
    let start_column = compact_item_view.range_anchor(range.start).start_column;
    let viewport_clamped = (plan.viewport_x - input.requested_viewport_x).abs() > f32::EPSILON;
    let rebuild_model = input.force_rebuild_model
        || !input.schedule_thumbnails
        || should_rebuild_virtual_cache(
            &input.cache,
            &plan,
            &item_view_layout,
            input.thumbnail_size_px,
        );

    if !rebuild_model {
        return VirtualViewSnapshotUpdate {
            entry_count: visible_count,
            layout: item_view_layout,
            viewport_x: plan.viewport_x,
            viewport_clamped,
            range: plan.range,
            visible_range: plan.visible_range,
            start_column: plan.start_column,
            entries: PaneEntryModel::default(),
            rebuild_model: false,
        };
    }

    let entries = snapshot_entries_range(&input, range.clone());

    VirtualViewSnapshotUpdate {
        entry_count: visible_count,
        layout: item_view_layout,
        viewport_x: plan.viewport_x,
        viewport_clamped,
        range,
        visible_range: plan.visible_range,
        start_column,
        entries,
        rebuild_model: true,
    }
}

fn cached_snapshot_layout(
    input: &VirtualViewSnapshotInput,
    visible_count: usize,
) -> Option<Arc<ItemViewLayoutEngine>> {
    let cached = input.cache.layout.as_ref()?;
    let compact = cached.as_compact();
    if compact.entry_count != visible_count {
        return None;
    }

    if main_layout_matches_cached_layout(&input.layout, compact) {
        return Some(Arc::clone(cached));
    }

    Some(Arc::new(ItemViewLayoutEngine::from(
        compact.relayout_with_main_layout(input.layout),
    )))
}

fn main_layout_matches_cached_layout(
    layout: &MainItemViewLayout,
    compact_item_view: &CompactItemViewLayout,
) -> bool {
    layout.rows_per_column == compact_item_view.rows_per_column
        && same_snapshot_layout_metric(layout.viewport_width, compact_item_view.viewport_width)
        && same_snapshot_layout_metric(layout.cell_width, compact_item_view.cell_width)
        && same_snapshot_layout_metric(layout.row_height, compact_item_view.row_height)
        && same_snapshot_layout_metric(layout.padding, compact_item_view.padding)
}

fn same_snapshot_layout_metric(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.5
}

fn should_rebuild_virtual_cache(
    cache: &VirtualViewCache,
    plan: &VirtualItemViewPlan,
    layout: &Arc<ItemViewLayoutEngine>,
    thumbnail_size_px: u32,
) -> bool {
    !cache.matches_layout_arc(layout, thumbnail_size_px)
        || !cached_range_covers_visible_range(cache, &plan.visible_range)
}

fn cached_range_covers_visible_range(
    cache: &VirtualViewCache,
    visible_range: &Range<usize>,
) -> bool {
    if visible_range.is_empty() {
        return cache.range.is_empty();
    }

    cache.range.start <= visible_range.start && cache.range.end >= visible_range.end
}

fn snapshot_visible_entry_count(input: &VirtualViewSnapshotInput) -> usize {
    if let Some(indices) = input.visible_entry_indices.as_ref() {
        return indices.len();
    }

    if snapshot_filters_are_identity(input) {
        return input.entries.len();
    }

    input
        .entries
        .iter()
        .filter(|entry| snapshot_matches_entry_filters(entry, input))
        .count()
}

fn snapshot_compact_item_view_layout(
    input: &VirtualViewSnapshotInput,
    visible_count: usize,
) -> CompactItemViewLayout {
    if let Some(indices) = input.visible_entry_indices.as_ref() {
        let widths = indices
            .iter()
            .take(visible_count)
            .map(|&index| {
                input
                    .entries
                    .get(index)
                    .map(ItemViewModelEntry::model_name_width_units)
                    .unwrap_or_default()
            })
            .chain(std::iter::repeat(0.0))
            .take(visible_count);
        input.layout.compact_item_view_from_text_width_units(widths)
    } else if snapshot_filters_are_identity(input) {
        let widths = input
            .entries
            .iter()
            .take(visible_count)
            .map(ItemViewModelEntry::model_name_width_units)
            .chain(std::iter::repeat(0.0))
            .take(visible_count);
        input.layout.compact_item_view_from_text_width_units(widths)
    } else {
        let widths = input
            .entries
            .iter()
            .filter(|entry| snapshot_matches_entry_filters(entry, input))
            .take(visible_count)
            .map(ItemViewModelEntry::model_name_width_units)
            .chain(std::iter::repeat(0.0))
            .take(visible_count);
        input.layout.compact_item_view_from_text_width_units(widths)
    }
}

fn snapshot_entries_range(input: &VirtualViewSnapshotInput, range: Range<usize>) -> PaneEntryModel {
    if range.is_empty() {
        return PaneEntryModel::default();
    }

    let mut previous_location =
        if input.visible_entries_have_locations && input.visible_location_groups.is_none() {
            snapshot_previous_visible_entry_location(input, range.start)
        } else {
            None
        };
    let mut snapshot_entries = Vec::with_capacity(range.end.saturating_sub(range.start));

    if let Some(indices) = input.visible_entry_indices.as_ref() {
        for (offset, &index) in indices
            .get(range.start..range.end.min(indices.len()))
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            snapshot_entries.push(snapshot_owned_entry(
                input,
                range.start + offset,
                &mut previous_location,
                input
                    .entries
                    .entry_arc(index)
                    .expect("visible entry index should reference a pane entry"),
            ));
        }
        return PaneEntryModel::new(snapshot_entries);
    }

    if !snapshot_filters_are_identity(input) {
        for (offset, entry) in input
            .entries
            .entry_arcs_range(0..input.entries.len())
            .filter(|entry| snapshot_matches_entry_filters(entry, input))
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
            .enumerate()
        {
            snapshot_entries.push(snapshot_owned_entry(
                input,
                range.start + offset,
                &mut previous_location,
                entry,
            ));
        }
        return PaneEntryModel::new(snapshot_entries);
    }

    for (offset, entry) in input
        .entries
        .entry_arcs_range(range.start..range.end)
        .enumerate()
    {
        snapshot_entries.push(snapshot_owned_entry(
            input,
            range.start + offset,
            &mut previous_location,
            entry,
        ));
    }

    PaneEntryModel::new(snapshot_entries)
}

fn snapshot_previous_visible_entry_location(
    input: &VirtualViewSnapshotInput,
    start_visible_index: usize,
) -> Option<String> {
    start_visible_index
        .checked_sub(1)
        .and_then(|index| snapshot_visible_entry_location_at(input, index))
}

fn snapshot_owned_entry(
    input: &VirtualViewSnapshotInput,
    visible_index: usize,
    previous_location: &mut Option<String>,
    entry: ItemViewModelEntryArc,
) -> ItemViewModelEntryArc {
    if !input.visible_entries_have_locations {
        return entry;
    }

    let group = if let Some(groups) = input.visible_location_groups.as_ref() {
        groups.get(visible_index).cloned().unwrap_or_default()
    } else if previous_location.as_deref() != Some(entry.model_location()) {
        search_group_label(entry.model_location())
    } else {
        String::new()
    };
    *previous_location = Some(entry.model_location().to_string());
    item_view_model_entry_with_group(entry, group)
}

fn snapshot_visible_entry_location_at(
    input: &VirtualViewSnapshotInput,
    visible_index: usize,
) -> Option<String> {
    if let Some(indices) = input.visible_entry_indices.as_ref() {
        return indices
            .get(visible_index)
            .map(|&entry_index| input.entries[entry_index].model_location().to_string());
    }

    if snapshot_filters_are_identity(input) {
        return input
            .entries
            .get(visible_index)
            .map(|entry| entry.model_location().to_string());
    }

    input
        .entries
        .iter()
        .filter(|entry| snapshot_matches_entry_filters(entry, input))
        .nth(visible_index)
        .map(|entry| entry.model_location().to_string())
}

fn search_group_label(location: &str) -> String {
    if location == "." {
        "Current folder".to_string()
    } else if location.is_empty() {
        "Unknown location".to_string()
    } else {
        location.to_string()
    }
}

fn snapshot_filters_are_identity(input: &VirtualViewSnapshotInput) -> bool {
    item_view_filters_are_identity(
        input.query.as_str(),
        input.kind_filter,
        input.modified_filter,
        input.size_filter,
        &input.chooser_patterns,
    )
}

fn snapshot_matches_entry_filters(
    entry: &(impl ItemViewModelEntry + ?Sized),
    input: &VirtualViewSnapshotInput,
) -> bool {
    item_view_entry_matches_filters(
        entry,
        input.query.as_str(),
        input.kind_filter,
        input.modified_filter,
        input.size_filter,
        &input.chooser_patterns,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileEntry;
    use crate::app::item_view_renderer::{
        ItemViewRenderGeometry, ItemViewRenderMetrics, ItemViewRenderPlanInput,
        decorate_render_plan,
    };
    use std::sync::Arc;

    fn layout() -> MainItemViewLayout {
        MainItemViewLayout {
            viewport_x: 0.0,
            viewport_width: 250.0,
            rows_per_column: 4,
            cell_width: 100.0,
            row_height: 90.0,
            padding: 10.0,
            item_padding: 0.0,
            media_width: 1.0,
            media_text_gap: 0.0,
            title_font_size: 1.0,
        }
    }

    fn snapshot_test_entry(index: usize, location: &str) -> FileEntry {
        let name = format!("item-{index}.txt");
        FileEntry {
            name: name.into(),
            path: format!("/tmp/item-{index}.txt").into(),
            group: "".into(),
            location: location.into(),
            kind: "File".into(),
            size: "1 KB".into(),
            size_bytes: 1024.0,
            modified: "Today".into(),
            modified_age_days: 0,
            is_dir: false,
        }
    }

    fn snapshot_entries(count: usize) -> PaneEntryModel {
        PaneEntryModel::from_entries(
            (0..count)
                .map(|index| snapshot_test_entry(index, ""))
                .collect::<Vec<_>>(),
        )
    }

    fn snapshot_input(
        entries: PaneEntryModel,
        requested_viewport_x: f32,
        cache: VirtualViewCache,
    ) -> VirtualViewSnapshotInput {
        VirtualViewSnapshotInput {
            layout: layout(),
            requested_viewport_x,
            range_hint: None,
            thumbnail_size_px: 64,
            schedule_thumbnails: true,
            force_rebuild_model: false,
            visible_count_override: None,
            cache,
            entries,
            visible_entry_indices: None,
            visible_entries_have_locations: false,
            visible_location_groups: None,
            query: String::new(),
            kind_filter: 0,
            modified_filter: 0,
            size_filter: 0,
            chooser_patterns: Vec::new(),
        }
    }

    fn cache_for_layout(
        range: Range<usize>,
        entry_count: usize,
        thumbnail_size_px: u32,
    ) -> VirtualViewCache {
        let names = (0..entry_count)
            .map(|index| format!("item-{index}.txt"))
            .collect::<Vec<_>>();
        let mut cache = VirtualViewCache {
            range,
            ..VirtualViewCache::default()
        };
        cache.update_layout_signature_arc(
            Arc::new(ItemViewLayoutEngine::from(
                layout().compact_item_view_from_names(names.iter().map(String::as_str)),
            )),
            thumbnail_size_px,
        );
        cache
    }

    #[test]
    fn snapshot_update_reuses_model_inside_same_range() {
        let entries = snapshot_entries(100);

        let first = prepare_virtual_view_snapshot_update(snapshot_input(
            entries.clone(),
            0.0,
            VirtualViewCache::default(),
        ));
        assert!(first.rebuild_model);
        assert_eq!(first.range, 0..20);
        assert_eq!(first.entries.len(), 20);
        assert_eq!(first.viewport_x, 0.0);

        let second = prepare_virtual_view_snapshot_update(snapshot_input(
            entries,
            40.0,
            cache_for_layout(first.range.clone(), first.entry_count, 64),
        ));
        assert!(!second.rebuild_model);
        assert!(second.entries.is_empty());
        assert_eq!(second.viewport_x, 40.0);
    }

    #[test]
    fn snapshot_update_force_rebuilds_model_inside_cached_range() {
        let entries = snapshot_entries(100);

        let first = prepare_virtual_view_snapshot_update(snapshot_input(
            entries.clone(),
            0.0,
            VirtualViewCache::default(),
        ));
        assert!(first.rebuild_model);

        let mut input = snapshot_input(
            entries,
            40.0,
            cache_for_layout(first.range.clone(), first.entry_count, 64),
        );
        input.force_rebuild_model = true;
        let second = prepare_virtual_view_snapshot_update(input);

        assert!(second.rebuild_model);
        assert!(!second.entries.is_empty());
        assert_eq!(second.viewport_x, 40.0);
    }

    #[test]
    fn snapshot_update_reuses_model_while_cached_range_covers_visible_range() {
        let entries = snapshot_entries(160);

        let first = prepare_virtual_view_snapshot_update(snapshot_input(
            entries.clone(),
            0.0,
            VirtualViewCache::default(),
        ));
        assert!(first.rebuild_model);
        assert_eq!(first.range, 0..20);
        assert_eq!(first.visible_range, 0..12);

        let second = prepare_virtual_view_snapshot_update(snapshot_input(
            entries,
            115.0,
            cache_for_layout(first.range.clone(), first.entry_count, 64),
        ));
        assert_eq!(second.visible_range, 4..16);
        assert_eq!(second.range, 0..24);
        assert!(!second.rebuild_model);
        assert!(second.entries.is_empty());
    }

    #[test]
    fn snapshot_update_reuses_layout_when_thumbnail_size_changes() {
        let entries = snapshot_entries(100);
        let cache = cache_for_layout(0..20, entries.len(), 32);
        let cached_layout = Arc::clone(cache.layout.as_ref().expect("layout should be cached"));

        let update = prepare_virtual_view_snapshot_update(snapshot_input(entries, 40.0, cache));

        assert!(update.rebuild_model);
        assert!(!update.entries.is_empty());
        assert!(Arc::ptr_eq(&update.layout, &cached_layout));
    }

    #[test]
    fn snapshot_update_relayouts_cached_size_hints_when_zoom_metrics_change() {
        let entries = snapshot_entries(100);
        let first = prepare_virtual_view_snapshot_update(snapshot_input(
            entries.clone(),
            0.0,
            VirtualViewCache::default(),
        ));
        let cached_layout = Arc::clone(&first.layout);
        let mut cache = VirtualViewCache {
            range: first.range,
            ..VirtualViewCache::default()
        };
        cache.update_layout_signature_arc(Arc::clone(&cached_layout), 64);
        let mut input = snapshot_input(entries, 0.0, cache);
        input.layout.rows_per_column = 2;
        input.layout.cell_width = 148.0;
        input.layout.row_height = 128.0;
        input.thumbnail_size_px = 128;
        let expected = cached_layout
            .as_compact()
            .relayout_with_main_layout(input.layout);

        let update = prepare_virtual_view_snapshot_update(input);

        assert!(update.rebuild_model);
        assert!(!update.entries.is_empty());
        assert!(!Arc::ptr_eq(&update.layout, &cached_layout));
        assert!(
            update
                .layout
                .as_compact()
                .matches_layout_signature(&expected)
        );
        assert_eq!(update.layout.as_compact().rows_per_column, 2);
        assert_eq!(update.layout.as_compact().cell_width, 148.0);
    }

    #[test]
    fn snapshot_update_expands_rebuild_range_with_aligned_zoom_hint() {
        let entries = snapshot_entries(100);
        let mut input = snapshot_input(entries, 0.0, VirtualViewCache::default());
        input.range_hint = Some(0..35);

        let update = prepare_virtual_view_snapshot_update(input);

        assert!(update.rebuild_model);
        assert_eq!(update.visible_range, 0..12);
        assert_eq!(update.range, 0..36);
        assert_eq!(update.start_column, 0);
        assert_eq!(update.entries.len(), 36);
    }

    #[test]
    fn snapshot_update_clamps_out_of_bounds_viewport() {
        let update = prepare_virtual_view_snapshot_update(snapshot_input(
            snapshot_entries(10),
            800.0,
            VirtualViewCache::default(),
        ));

        assert!(update.viewport_clamped);
        assert_eq!(update.viewport_x, 86.0);
        assert_eq!(update.range, 0..10);
    }

    #[test]
    fn virtual_snapshot_entries_keep_names_and_renderable_title_geometry() {
        let update = prepare_virtual_view_snapshot_update(snapshot_input(
            snapshot_entries(24),
            115.0,
            VirtualViewCache::default(),
        ));
        assert!(update.rebuild_model);

        let mut entries = update
            .entries
            .iter()
            .map(ItemViewModelEntry::model_to_item_view_entry)
            .collect::<Vec<_>>();
        let input = ItemViewRenderPlanInput {
            cell_width: update.layout.layout_metrics().cell_width,
            render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 1),
            show_location: false,
        };
        decorate_render_plan(&mut entries, input);
        let geometry = ItemViewRenderGeometry::from_plan_input(input);

        assert!(entries.iter().all(|entry| !entry.name.is_empty()));
        assert!(geometry.text_width > 0.0);
        assert!(geometry.title_y >= 0.0);
        assert!(geometry.title_line_height > 0.0);
        assert!(geometry.title_font_size > 0.0);
    }

    #[test]
    fn snapshot_update_uses_precomputed_visible_location_groups() {
        let entries = (0..24)
            .map(|index| {
                snapshot_test_entry(
                    index,
                    if index < 8 {
                        "/search/location-a"
                    } else {
                        "/search/location-b"
                    },
                )
            })
            .collect::<Vec<_>>();
        let groups = (0..entries.len())
            .map(|index| format!("cached-group-{index}"))
            .collect::<Vec<_>>();

        let update = prepare_virtual_view_snapshot_update(VirtualViewSnapshotInput {
            layout: layout(),
            requested_viewport_x: 360.0,
            range_hint: None,
            thumbnail_size_px: 64,
            schedule_thumbnails: true,
            force_rebuild_model: false,
            visible_count_override: None,
            cache: VirtualViewCache::default(),
            entries: PaneEntryModel::from_entries(entries),
            visible_entry_indices: None,
            visible_entries_have_locations: true,
            visible_location_groups: Some(Arc::from(groups)),
            query: String::new(),
            kind_filter: 0,
            modified_filter: 0,
            size_filter: 0,
            chooser_patterns: Vec::new(),
        });

        assert!(update.rebuild_model);
        assert_eq!(update.range.start, 4);
        assert_eq!(update.entries[0].model_group(), "cached-group-4");
        assert_eq!(update.entries[1].model_group(), "cached-group-5");
        assert_eq!(update.entries[4].model_group(), "cached-group-8");
        assert_ne!(update.entries[0].model_group(), "/search/location-a");
    }
}
