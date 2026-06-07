use crate::app::geometry::{
    CompactItemViewLayout, ITEM_VIEW_OVERSCAN_COLUMNS, ItemViewLayoutEngine, ItemViewLayouter,
    MainItemViewLayout, VirtualItemViewPlan,
};
use crate::app::pane::{PaneEntrySnapshot, VirtualViewCache};
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
    pub(crate) entries: Vec<PaneEntrySnapshot>,
    pub(crate) rebuild_model: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualViewSnapshotInput {
    pub(crate) layout: MainItemViewLayout,
    pub(crate) requested_viewport_x: f32,
    pub(crate) range_hint: Option<Range<usize>>,
    pub(crate) thumbnail_size_px: u32,
    pub(crate) schedule_thumbnails: bool,
    pub(crate) visible_count_override: Option<usize>,
    pub(crate) cache: VirtualViewCache,
    pub(crate) entries: Arc<[PaneEntrySnapshot]>,
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
    let rebuild_model = !input.schedule_thumbnails
        || should_rebuild_virtual_cache(
            &input.cache,
            &plan,
            item_view_layout.as_ref(),
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
            entries: Vec::new(),
            rebuild_model: false,
        };
    }

    let mut entries = snapshot_entries_range(&input, range.clone());
    annotate_snapshot_location_groups(&input, range.start, &mut entries);

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
    if input.cache.range.is_empty() || input.cache.thumbnail_size_px != input.thumbnail_size_px {
        return None;
    }

    let cached = input.cache.layout.as_ref()?;
    let compact = cached.as_compact();
    if compact.entry_count != visible_count
        || !main_layout_matches_cached_layout(&input.layout, compact)
    {
        return None;
    }

    Some(Arc::clone(cached))
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
    layout: &ItemViewLayoutEngine,
    thumbnail_size_px: u32,
) -> bool {
    !cache.matches_layout(layout, thumbnail_size_px)
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
                    .map(|entry| entry.name_width_units)
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
            .map(|entry| entry.name_width_units)
            .chain(std::iter::repeat(0.0))
            .take(visible_count);
        input.layout.compact_item_view_from_text_width_units(widths)
    } else {
        let widths = input
            .entries
            .iter()
            .filter(|entry| snapshot_matches_entry_filters(entry, input))
            .take(visible_count)
            .map(|entry| entry.name_width_units)
            .chain(std::iter::repeat(0.0))
            .take(visible_count);
        input.layout.compact_item_view_from_text_width_units(widths)
    }
}

fn snapshot_entries_range(
    input: &VirtualViewSnapshotInput,
    range: Range<usize>,
) -> Vec<PaneEntrySnapshot> {
    if range.is_empty() {
        return Vec::new();
    }

    if let Some(indices) = input.visible_entry_indices.as_ref() {
        return indices
            .get(range.start..range.end.min(indices.len()))
            .unwrap_or(&[])
            .iter()
            .map(|&index| input.entries[index].clone())
            .collect();
    }

    if !snapshot_filters_are_identity(input) {
        return input
            .entries
            .iter()
            .filter(|entry| snapshot_matches_entry_filters(entry, input))
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
            .cloned()
            .collect();
    }

    input
        .entries
        .get(range.start..range.end.min(input.entries.len()))
        .unwrap_or(&[])
        .to_vec()
}

fn annotate_snapshot_location_groups(
    input: &VirtualViewSnapshotInput,
    start_visible_index: usize,
    entries: &mut [PaneEntrySnapshot],
) {
    if !input.visible_entries_have_locations {
        return;
    }

    if let Some(groups) = input.visible_location_groups.as_ref() {
        for (offset, entry) in entries.iter_mut().enumerate() {
            entry.group = groups
                .get(start_visible_index + offset)
                .cloned()
                .unwrap_or_default();
        }
        return;
    }

    let mut previous_location = start_visible_index
        .checked_sub(1)
        .and_then(|index| snapshot_visible_entry_location_at(input, index));
    for entry in entries {
        if previous_location.as_deref() != Some(entry.location.as_str()) {
            entry.group = search_group_label(entry.location.as_str());
        } else {
            entry.group.clear();
        }
        previous_location = Some(entry.location.clone());
    }
}

fn snapshot_visible_entry_location_at(
    input: &VirtualViewSnapshotInput,
    visible_index: usize,
) -> Option<String> {
    if let Some(indices) = input.visible_entry_indices.as_ref() {
        return indices
            .get(visible_index)
            .map(|&entry_index| input.entries[entry_index].location.clone());
    }

    if snapshot_filters_are_identity(input) {
        return input
            .entries
            .get(visible_index)
            .map(|entry| entry.location.clone());
    }

    input
        .entries
        .iter()
        .filter(|entry| snapshot_matches_entry_filters(entry, input))
        .nth(visible_index)
        .map(|entry| entry.location.clone())
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
    input.query.is_empty()
        && input.kind_filter == 0
        && input.modified_filter == 0
        && input.size_filter == 0
        && input.chooser_patterns.is_empty()
}

fn snapshot_matches_entry_filters(
    entry: &PaneEntrySnapshot,
    input: &VirtualViewSnapshotInput,
) -> bool {
    snapshot_matches_search_query(entry, input.query.as_str())
        && snapshot_matches_kind_filter(entry, input.kind_filter)
        && snapshot_matches_modified_filter(entry, input.modified_filter)
        && snapshot_matches_size_filter(entry, input.size_filter)
        && snapshot_matches_chooser_filter(entry, &input.chooser_patterns)
}

fn snapshot_matches_search_query(entry: &PaneEntrySnapshot, query: &str) -> bool {
    query.is_empty()
        || entry.name.to_ascii_lowercase().contains(query)
        || entry.path.to_ascii_lowercase().contains(query)
}

fn snapshot_matches_kind_filter(entry: &PaneEntrySnapshot, filter: i32) -> bool {
    match filter {
        1 => entry.is_dir,
        2 => !entry.is_dir,
        3 => !entry.is_dir && snapshot_is_image_path(entry.path.as_str()),
        _ => true,
    }
}

fn snapshot_matches_modified_filter(entry: &PaneEntrySnapshot, filter: i32) -> bool {
    match filter {
        1 => entry.modified_age_days == 0,
        2 => entry.modified_age_days >= 0 && entry.modified_age_days <= 7,
        3 => entry.modified_age_days >= 0 && entry.modified_age_days <= 30,
        _ => true,
    }
}

fn snapshot_matches_size_filter(entry: &PaneEntrySnapshot, filter: i32) -> bool {
    if entry.is_dir {
        return filter == 0;
    }

    match filter {
        1 => entry.size_bytes < 1_048_576.0,
        2 => entry.size_bytes >= 1_048_576.0 && entry.size_bytes <= 104_857_600.0,
        3 => entry.size_bytes > 104_857_600.0,
        _ => true,
    }
}

fn snapshot_matches_chooser_filter(entry: &PaneEntrySnapshot, patterns: &[String]) -> bool {
    entry.is_dir
        || patterns.is_empty()
        || patterns
            .iter()
            .any(|pattern| snapshot_glob_matches(pattern, entry.name.as_str()))
}

fn snapshot_is_image_path(path: &str) -> bool {
    let Some(extension) = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "avif" | "bmp" | "gif" | "heic" | "heif" | "jpeg" | "jpg" | "png" | "svg" | "webp"
    )
}

fn snapshot_glob_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    snapshot_glob_matches_bytes(pattern.as_bytes(), text.as_bytes())
}

fn snapshot_glob_matches_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let (mut pattern_index, mut text_index) = (0usize, 0usize);
    let mut star_index = None;
    let mut star_text_index = 0usize;

    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            pattern_index += 1;
            text_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_text_index = text_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_text_index += 1;
            text_index = star_text_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn snapshot_test_entry(index: usize, location: &str) -> PaneEntrySnapshot {
        let name = format!("item-{index}.txt");
        PaneEntrySnapshot {
            name_width_units: crate::app::geometry::compact_text_width_units(&name),
            name,
            path: format!("/tmp/item-{index}.txt"),
            group: String::new(),
            location: location.to_string(),
            kind: "File".to_string(),
            size: "1 KB".to_string(),
            size_bytes: 1024.0,
            modified: "Today".to_string(),
            modified_age_days: 0,
            is_dir: false,
        }
    }

    fn snapshot_entries(count: usize) -> Arc<[PaneEntrySnapshot]> {
        Arc::from(
            (0..count)
                .map(|index| snapshot_test_entry(index, ""))
                .collect::<Vec<_>>(),
        )
    }

    fn snapshot_input(
        entries: Arc<[PaneEntrySnapshot]>,
        requested_viewport_x: f32,
        cache: VirtualViewCache,
    ) -> VirtualViewSnapshotInput {
        VirtualViewSnapshotInput {
            layout: layout(),
            requested_viewport_x,
            range_hint: None,
            thumbnail_size_px: 64,
            schedule_thumbnails: true,
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
        cache.update_layout_signature(
            layout()
                .compact_item_view_from_names(names.iter().map(String::as_str))
                .into(),
            thumbnail_size_px,
        );
        cache
    }

    #[test]
    fn snapshot_update_reuses_model_inside_same_range() {
        let entries = snapshot_entries(100);

        let first = prepare_virtual_view_snapshot_update(snapshot_input(
            Arc::clone(&entries),
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
    fn snapshot_update_reuses_model_while_cached_range_covers_visible_range() {
        let entries = snapshot_entries(160);

        let first = prepare_virtual_view_snapshot_update(snapshot_input(
            Arc::clone(&entries),
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
            .map(PaneEntrySnapshot::to_item_view_entry)
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
            visible_count_override: None,
            cache: VirtualViewCache::default(),
            entries: Arc::from(entries),
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
        assert_eq!(update.entries[0].group, "cached-group-4");
        assert_eq!(update.entries[1].group, "cached-group-5");
        assert_eq!(update.entries[4].group, "cached-group-8");
        assert_ne!(update.entries[0].group, "/search/location-a");
    }
}
