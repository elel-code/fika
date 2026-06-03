use crate::FileEntry;
use crate::app::geometry::{RectBounds, SelectionRect};
use crate::app::state::AppState;
use std::ops::Range;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FilteredEntrySummary {
    pub(crate) count: usize,
    pub(crate) folders: usize,
    pub(crate) files: usize,
    pub(crate) has_locations: bool,
    pub(crate) visible_paths: Option<Vec<String>>,
}

pub(crate) fn entries_have_locations(entries: &[FileEntry]) -> bool {
    entries.iter().any(|entry| !entry.location.is_empty())
}

pub(crate) fn retained_visible_paths(
    selected_paths: &[String],
    visible_paths: &[String],
) -> Vec<String> {
    selected_paths
        .iter()
        .filter(|selected| visible_paths.iter().any(|visible| visible == *selected))
        .cloned()
        .collect()
}

pub(crate) fn filtered_entry_paths(state: &AppState) -> Vec<String> {
    if let Some(indices) = state.panes.active().search.visible_entry_indices.as_ref() {
        return indices
            .iter()
            .filter_map(|index| state.panes.active().entries.get(*index))
            .map(|entry| entry.path.to_string())
            .collect();
    }

    if filters_are_identity(state) {
        return state
            .panes
            .active()
            .entries
            .iter()
            .map(|entry| entry.path.to_string())
            .collect();
    }

    filtered_entries(state)
        .iter()
        .map(|entry| entry.path.to_string())
        .collect()
}

pub(crate) fn filtered_entries(state: &AppState) -> Vec<FileEntry> {
    let query = state.panes.active().search.query.to_ascii_lowercase();
    state
        .panes
        .active()
        .entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, state, &query))
        .cloned()
        .collect()
}

pub(crate) fn filtered_entry_count(state: &AppState) -> usize {
    if let Some(indices) = state.panes.active().search.visible_entry_indices.as_ref() {
        return indices.len();
    }

    if filters_are_identity(state) {
        return state.panes.active().entries.len();
    }

    let query = state.panes.active().search.query.to_ascii_lowercase();
    state
        .panes
        .active()
        .entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, state, &query))
        .count()
}

pub(crate) fn filtered_entry_at(state: &AppState, index: usize) -> Option<FileEntry> {
    if let Some(indices) = state.panes.active().search.visible_entry_indices.as_ref() {
        return indices
            .get(index)
            .and_then(|entry_index| state.panes.active().entries.get(*entry_index))
            .cloned();
    }

    if filters_are_identity(state) {
        return state.panes.active().entries.get(index).cloned();
    }

    let query = state.panes.active().search.query.to_ascii_lowercase();
    state
        .panes
        .active()
        .entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, state, &query))
        .nth(index)
        .cloned()
}

pub(crate) fn rebuild_visible_entry_index(
    state: &mut AppState,
    collect_paths: bool,
) -> FilteredEntrySummary {
    if filters_are_identity(state) {
        state.panes.active_mut().search.visible_entry_indices = None;
        let mut summary = FilteredEntrySummary {
            count: state.panes.active().entries.len(),
            visible_paths: collect_paths.then(Vec::new),
            ..FilteredEntrySummary::default()
        };
        for entry in &state.panes.active().entries {
            if entry.is_dir {
                summary.folders += 1;
            } else {
                summary.files += 1;
            }
            summary.has_locations |= !entry.location.is_empty();
            if let Some(paths) = summary.visible_paths.as_mut() {
                paths.push(entry.path.to_string());
            }
        }
        state
            .panes
            .active_mut()
            .search
            .visible_entries_have_locations = summary.has_locations;
        return summary;
    }

    let query = state.panes.active().search.query.to_ascii_lowercase();
    let mut summary = FilteredEntrySummary {
        visible_paths: collect_paths.then(Vec::new),
        ..FilteredEntrySummary::default()
    };
    let mut indices = Vec::new();

    for (index, entry) in state
        .panes
        .active()
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches_entry_filters(entry, state, &query))
    {
        summary.count += 1;
        if entry.is_dir {
            summary.folders += 1;
        } else {
            summary.files += 1;
        }
        summary.has_locations |= !entry.location.is_empty();
        if let Some(paths) = summary.visible_paths.as_mut() {
            paths.push(entry.path.to_string());
        }
        indices.push(index);
    }

    {
        let search = &mut state.panes.active_mut().search;
        search.visible_entry_indices = Some(indices);
        search.visible_entries_have_locations = summary.has_locations;
    }
    summary
}

pub(crate) fn filtered_entry_summary(
    state: &AppState,
    collect_paths: bool,
) -> FilteredEntrySummary {
    let query = state.panes.active().search.query.to_ascii_lowercase();
    let mut summary = FilteredEntrySummary {
        visible_paths: collect_paths.then(Vec::new),
        ..FilteredEntrySummary::default()
    };

    for entry in state
        .panes
        .active()
        .entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, state, &query))
    {
        summary.count += 1;
        if entry.is_dir {
            summary.folders += 1;
        } else {
            summary.files += 1;
        }
        summary.has_locations |= !entry.location.is_empty();
        if let Some(paths) = summary.visible_paths.as_mut() {
            paths.push(entry.path.to_string());
        }
    }

    summary
}

#[cfg(test)]
pub(crate) fn filtered_entries_range(state: &AppState, range: Range<usize>) -> Vec<FileEntry> {
    if range.is_empty() {
        return Vec::new();
    }

    let mut entries =
        if let Some(indices) = state.panes.active().search.visible_entry_indices.as_ref() {
            indices
                .get(range.start..range.end.min(indices.len()))
                .unwrap_or(&[])
                .iter()
                .filter_map(|index| state.panes.active().entries.get(*index))
                .cloned()
                .collect()
        } else if filters_are_identity(state) {
            state
                .panes
                .active()
                .entries
                .get(range.start..range.end.min(state.panes.active().entries.len()))
                .unwrap_or(&[])
                .to_vec()
        } else {
            let query = state.panes.active().search.query.to_ascii_lowercase();
            state
                .panes
                .active()
                .entries
                .iter()
                .filter(|entry| matches_entry_filters(entry, state, &query))
                .skip(range.start)
                .take(range.end.saturating_sub(range.start))
                .cloned()
                .collect()
        };

    annotate_visible_location_groups(state, range.start, &mut entries);
    entries
}

#[cfg(test)]
fn annotate_visible_location_groups(
    state: &AppState,
    start_visible_index: usize,
    entries: &mut [FileEntry],
) {
    if !state.panes.active().search.visible_entries_have_locations {
        return;
    }

    let mut previous_location = start_visible_index
        .checked_sub(1)
        .and_then(|index| visible_entry_location_at(state, index));
    for entry in entries {
        if previous_location.as_deref() != Some(entry.location.as_str()) {
            entry.group = search_group_label(entry.location.as_str()).into();
        } else {
            entry.group = String::new().into();
        }
        previous_location = Some(entry.location.to_string());
    }
}

#[cfg(test)]
fn visible_entry_location_at(state: &AppState, visible_index: usize) -> Option<String> {
    if let Some(indices) = state.panes.active().search.visible_entry_indices.as_ref() {
        return indices
            .get(visible_index)
            .and_then(|entry_index| state.panes.active().entries.get(*entry_index))
            .map(|entry| entry.location.to_string());
    }

    if filters_are_identity(state) {
        return state
            .panes
            .active()
            .entries
            .get(visible_index)
            .map(|entry| entry.location.to_string());
    }

    let query = state.panes.active().search.query.to_ascii_lowercase();
    state
        .panes
        .active()
        .entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, state, &query))
        .nth(visible_index)
        .map(|entry| entry.location.to_string())
}

#[cfg(test)]
fn search_group_label(location: &str) -> String {
    if location == "." {
        "Current folder".to_string()
    } else if location.is_empty() {
        "Unknown location".to_string()
    } else {
        location.to_string()
    }
}

fn filters_are_identity(state: &AppState) -> bool {
    state.panes.active().search.query.is_empty()
        && state.panes.active().search.kind_filter == 0
        && state.panes.active().search.modified_filter == 0
        && state.panes.active().search.size_filter == 0
        && chooser_filter_is_identity(state)
}

fn chooser_filter_is_identity(state: &AppState) -> bool {
    state
        .chooser_filters
        .get(state.chooser_filter_index)
        .is_none_or(|filter| filter.patterns.is_empty())
}

fn matches_entry_filters(entry: &FileEntry, state: &AppState, query: &str) -> bool {
    matches_search_query(entry, query)
        && matches_kind_filter(entry, state.panes.active().search.kind_filter)
        && matches_modified_filter(entry, state.panes.active().search.modified_filter)
        && matches_size_filter(entry, state.panes.active().search.size_filter)
        && matches_chooser_filter(entry, state)
}

fn matches_search_query(entry: &FileEntry, query: &str) -> bool {
    query.is_empty()
        || entry.name.to_ascii_lowercase().contains(query)
        || entry.path.to_ascii_lowercase().contains(query)
}

fn matches_kind_filter(entry: &FileEntry, filter: i32) -> bool {
    match filter {
        1 => entry.is_dir,
        2 => !entry.is_dir,
        3 => !entry.is_dir && is_image_path(entry.path.as_str()),
        _ => true,
    }
}

fn matches_modified_filter(entry: &FileEntry, filter: i32) -> bool {
    match filter {
        1 => entry.modified_age_days == 0,
        2 => entry.modified_age_days >= 0 && entry.modified_age_days <= 7,
        3 => entry.modified_age_days >= 0 && entry.modified_age_days <= 30,
        _ => true,
    }
}

fn matches_size_filter(entry: &FileEntry, filter: i32) -> bool {
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

pub(crate) fn matches_chooser_filter(entry: &FileEntry, state: &AppState) -> bool {
    if entry.is_dir || state.chooser_filters.is_empty() {
        return true;
    }

    let Some(filter) = state.chooser_filters.get(state.chooser_filter_index) else {
        return true;
    };
    if filter.patterns.is_empty() {
        return true;
    }

    filter
        .patterns
        .iter()
        .any(|pattern| glob_matches(pattern, entry.name.as_str()))
}

fn is_image_path(path: &str) -> bool {
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

fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    glob_matches_bytes(pattern.as_bytes(), text.as_bytes())
}

fn glob_matches_bytes(pattern: &[u8], text: &[u8]) -> bool {
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
pub(crate) fn selection_range_paths(
    visible_paths: &[String],
    anchor: &str,
    target: &str,
) -> Vec<String> {
    let Some(anchor_index) = visible_paths.iter().position(|path| path == anchor) else {
        return vec![target.to_string()];
    };
    let Some(target_index) = visible_paths.iter().position(|path| path == target) else {
        return vec![target.to_string()];
    };
    let start = anchor_index.min(target_index);
    let end = anchor_index.max(target_index);
    visible_paths[start..=end].to_vec()
}

pub(crate) fn selection_range_paths_filtered(
    state: &AppState,
    anchor: &str,
    target: &str,
) -> Vec<String> {
    if anchor == target {
        return vec![target.to_string()];
    }

    let mut collecting = false;
    let mut found_anchor = false;
    let mut found_target = false;
    let mut range = Vec::new();

    for entry in visible_entry_iter(state) {
        let path = entry.path.as_str();
        let is_anchor = path == anchor;
        let is_target = path == target;

        if is_anchor {
            found_anchor = true;
        }
        if is_target {
            found_target = true;
        }

        if !collecting && (is_anchor || is_target) {
            collecting = true;
        }

        if collecting {
            range.push(path.to_string());
        }

        if collecting && (is_anchor || is_target) && found_anchor && found_target {
            return range;
        }
    }

    vec![target.to_string()]
}

pub(crate) fn selection_rect_paths(entries: &[FileEntry], rect: SelectionRect) -> Vec<String> {
    let rows_per_column = rect.rows_per_column.max(1) as usize;
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let column = index / rows_per_column;
            let row = index % rows_per_column;
            let tile_x1 = rect.padding + column as f32 * rect.cell_width;
            let tile_y1 = rect.padding + row as f32 * rect.row_height;
            let tile_x2 = tile_x1 + (rect.cell_width - 12.0).max(1.0);
            let tile_y2 = tile_y1 + rect.row_height.max(1.0);
            if RectBounds::new(rect.x1, rect.y1, rect.x2, rect.y2)
                .intersects(RectBounds::new(tile_x1, tile_y1, tile_x2, tile_y2))
            {
                Some(entry.path.to_string())
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn selection_rect_paths_filtered(state: &AppState, rect: SelectionRect) -> Vec<String> {
    let rows_per_column = rect.rows_per_column.max(1) as usize;
    let selection_bounds = RectBounds::new(rect.x1, rect.y1, rect.x2, rect.y2);
    let mut selected = Vec::new();
    let visible_range = selection_rect_visible_range(state, rect);

    for (visible_index, entry) in visible_entries_range_iter(state, visible_range) {
        let column = visible_index / rows_per_column;
        let row = visible_index % rows_per_column;
        let tile_x1 = rect.padding + column as f32 * rect.cell_width;
        let tile_y1 = rect.padding + row as f32 * rect.row_height;
        let tile_x2 = tile_x1 + (rect.cell_width - 12.0).max(1.0);
        let tile_y2 = tile_y1 + rect.row_height.max(1.0);
        if selection_bounds.intersects(RectBounds::new(tile_x1, tile_y1, tile_x2, tile_y2)) {
            selected.push(entry.path.to_string());
        }
    }

    selected
}

fn selection_rect_visible_range(state: &AppState, rect: SelectionRect) -> Range<usize> {
    let visible_count = filtered_entry_count(state);
    if visible_count == 0 {
        return 0..0;
    }

    let rows_per_column = rect.rows_per_column.max(1) as usize;
    let cell_width = rect.cell_width.max(1.0);
    let tile_width = (cell_width - 12.0).max(1.0);

    let first_column = ((rect.x1 - rect.padding - tile_width) / cell_width)
        .floor()
        .max(0.0) as usize;
    let last_column = ((rect.x2 - rect.padding) / cell_width).floor().max(0.0) as usize;

    let start = first_column
        .saturating_mul(rows_per_column)
        .min(visible_count);
    let end = ((last_column + 1).saturating_mul(rows_per_column)).min(visible_count);
    start..end.max(start)
}

fn visible_entries_range_iter(
    state: &AppState,
    range: Range<usize>,
) -> Box<dyn Iterator<Item = (usize, &FileEntry)> + '_> {
    if range.is_empty() {
        return Box::new(std::iter::empty());
    }

    if let Some(indices) = state.panes.active().search.visible_entry_indices.as_ref() {
        let start = range.start.min(indices.len());
        let end = range.end.min(indices.len());
        return Box::new(indices[start..end].iter().enumerate().filter_map(
            move |(offset, index)| {
                state
                    .panes
                    .active()
                    .entries
                    .get(*index)
                    .map(|entry| (start + offset, entry))
            },
        ));
    }

    if filters_are_identity(state) {
        let start = range.start.min(state.panes.active().entries.len());
        let end = range.end.min(state.panes.active().entries.len());
        return Box::new(
            state.panes.active().entries[start..end]
                .iter()
                .enumerate()
                .map(move |(offset, entry)| (start + offset, entry)),
        );
    }

    let query = state.panes.active().search.query.to_ascii_lowercase();
    Box::new(
        state
            .panes
            .active()
            .entries
            .iter()
            .filter(move |entry| matches_entry_filters(entry, state, &query))
            .enumerate()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start)),
    )
}

fn visible_entry_iter(state: &AppState) -> Box<dyn Iterator<Item = &FileEntry> + '_> {
    if let Some(indices) = state.panes.active().search.visible_entry_indices.as_ref() {
        return Box::new(
            indices
                .iter()
                .filter_map(|index| state.panes.active().entries.get(*index)),
        );
    }

    if filters_are_identity(state) {
        return Box::new(state.panes.active().entries.iter());
    }

    let query = state.panes.active().search.query.to_ascii_lowercase();
    Box::new(
        state
            .panes
            .active()
            .entries
            .iter()
            .filter(move |entry| matches_entry_filters(entry, state, &query)),
    )
}

pub(crate) fn append_unique_paths(selected_paths: &mut Vec<String>, paths: Vec<String>) {
    for path in paths {
        if !selected_paths.iter().any(|selected| selected == &path) {
            selected_paths.push(path);
        }
    }
}
