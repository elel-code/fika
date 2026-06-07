use crate::FileEntry;
use crate::app::item_view::SelectionRect;
use crate::app::pane::{PaneEntrySnapshot, PaneSearch, PaneState};
use crate::app::state::AppState;
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FilteredEntrySummary {
    pub(crate) count: usize,
    pub(crate) folders: usize,
    pub(crate) files: usize,
    pub(crate) has_locations: bool,
    pub(crate) visible_paths: Option<Vec<String>>,
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

pub(crate) fn filtered_entry_paths_for_slot(state: &AppState, slot: i32) -> Vec<String> {
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return Vec::new();
    };
    filtered_entry_paths_for_pane(state, pane)
}

pub(crate) fn filtered_entry_paths_for_pane(state: &AppState, pane: &PaneState) -> Vec<String> {
    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        return indices
            .iter()
            .map(|&index| pane.entries[index].path.clone())
            .collect();
    }

    let chooser_patterns = active_chooser_patterns(state);
    if filters_are_identity(&pane.search, &chooser_patterns) {
        return pane
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
    }

    filtered_entries_for_pane(state, pane)
        .iter()
        .map(|entry| entry.path.to_string())
        .collect()
}

#[cfg(test)]
pub(crate) fn filtered_entry_paths(state: &AppState) -> Vec<String> {
    filtered_entry_paths_for_slot(state, 0)
}

#[allow(dead_code)]
pub(crate) fn filtered_entries_for_slot(state: &AppState, slot: i32) -> Vec<FileEntry> {
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return Vec::new();
    };
    filtered_entries_for_pane(state, pane)
}

pub(crate) fn filtered_entries_for_pane(state: &AppState, pane: &PaneState) -> Vec<FileEntry> {
    let chooser_patterns = active_chooser_patterns(state);
    let query = pane.search.query.to_ascii_lowercase();
    pane.entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, &pane.search, &chooser_patterns, &query))
        .map(PaneEntrySnapshot::to_file_entry)
        .collect()
}

#[allow(dead_code)]
pub(crate) fn filtered_entries(state: &AppState) -> Vec<FileEntry> {
    filtered_entries_for_slot(state, 0)
}

pub(crate) fn filtered_entry_count_for_slot(state: &AppState, slot: i32) -> usize {
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return 0;
    };
    filtered_entry_count_for_pane(state, pane)
}

pub(crate) fn filtered_entry_count_for_pane(state: &AppState, pane: &PaneState) -> usize {
    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        return indices.len();
    }

    let chooser_patterns = active_chooser_patterns(state);
    if filters_are_identity(&pane.search, &chooser_patterns) {
        return pane.entries.len();
    }

    let query = pane.search.query.to_ascii_lowercase();
    pane.entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, &pane.search, &chooser_patterns, &query))
        .count()
}

pub(crate) fn filtered_entry_at_for_slot(
    state: &AppState,
    slot: i32,
    index: usize,
) -> Option<FileEntry> {
    let pane = state.panes.pane_for_slot(slot)?;
    filtered_entry_at_for_pane(state, pane, index)
}

pub(crate) fn filtered_entry_at_for_pane(
    state: &AppState,
    pane: &PaneState,
    index: usize,
) -> Option<FileEntry> {
    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        return indices
            .get(index)
            .map(|&entry_index| pane.entries[entry_index].to_file_entry());
    }

    let chooser_patterns = active_chooser_patterns(state);
    if filters_are_identity(&pane.search, &chooser_patterns) {
        return pane
            .entries
            .get(index)
            .map(PaneEntrySnapshot::to_file_entry);
    }

    let query = pane.search.query.to_ascii_lowercase();
    pane.entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, &pane.search, &chooser_patterns, &query))
        .nth(index)
        .map(PaneEntrySnapshot::to_file_entry)
}

pub(crate) fn filtered_entry_at(state: &AppState, index: usize) -> Option<FileEntry> {
    filtered_entry_at_for_slot(state, 0, index)
}

pub(crate) fn rebuild_visible_entry_index_for_slot(
    state: &mut AppState,
    slot: i32,
    collect_paths: bool,
) -> FilteredEntrySummary {
    let chooser_patterns = active_chooser_patterns(state);
    let Some(pane) = state.panes.pane_mut_for_slot(slot) else {
        return FilteredEntrySummary::default();
    };

    if filters_are_identity(&pane.search, &chooser_patterns) {
        pane.search.visible_entry_indices = None;
        pane.search.visible_location_groups = None;
        let mut summary = FilteredEntrySummary {
            count: pane.entries.len(),
            visible_paths: collect_paths.then(Vec::new),
            ..FilteredEntrySummary::default()
        };
        for entry in pane.entries.iter() {
            if entry.is_dir {
                summary.folders += 1;
            } else {
                summary.files += 1;
            }
            summary.has_locations |= !entry.location.is_empty();
            if let Some(paths) = summary.visible_paths.as_mut() {
                paths.push(entry.path.clone());
            }
        }
        pane.search.visible_entries_have_locations = summary.has_locations;
        if summary.has_locations {
            let groups =
                location_group_labels(pane.entries.iter().map(|entry| entry.location.as_str()));
            pane.search.visible_location_groups = Some(Arc::from(groups));
        }
        return summary;
    }

    let query = pane.search.query.to_ascii_lowercase();
    let mut summary = FilteredEntrySummary {
        visible_paths: collect_paths.then(Vec::new),
        ..FilteredEntrySummary::default()
    };
    let mut indices = Vec::new();
    let mut locations = Vec::new();

    for (index, entry) in
        pane.entries.iter().enumerate().filter(|(_, entry)| {
            matches_entry_filters(entry, &pane.search, &chooser_patterns, &query)
        })
    {
        summary.count += 1;
        if entry.is_dir {
            summary.folders += 1;
        } else {
            summary.files += 1;
        }
        summary.has_locations |= !entry.location.is_empty();
        if let Some(paths) = summary.visible_paths.as_mut() {
            paths.push(entry.path.clone());
        }
        locations.push(entry.location.clone());
        indices.push(index);
    }

    pane.search.visible_entry_indices = Some(Arc::from(indices));
    pane.search.visible_entries_have_locations = summary.has_locations;
    pane.search.visible_location_groups = summary
        .has_locations
        .then(|| Arc::from(location_group_labels(locations.iter().map(String::as_str))));
    summary
}

#[allow(dead_code)]
pub(crate) fn filtered_entry_summary(
    state: &AppState,
    collect_paths: bool,
) -> FilteredEntrySummary {
    filtered_entry_summary_for_slot(state, 0, collect_paths)
}

#[allow(dead_code)]
pub(crate) fn filtered_entry_summary_for_slot(
    state: &AppState,
    slot: i32,
    collect_paths: bool,
) -> FilteredEntrySummary {
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return FilteredEntrySummary::default();
    };
    filtered_entry_summary_for_pane(state, pane, collect_paths)
}

pub(crate) fn filtered_entry_summary_for_pane(
    state: &AppState,
    pane: &PaneState,
    collect_paths: bool,
) -> FilteredEntrySummary {
    let chooser_patterns = active_chooser_patterns(state);
    let query = pane.search.query.to_ascii_lowercase();
    let mut summary = FilteredEntrySummary {
        visible_paths: collect_paths.then(Vec::new),
        ..FilteredEntrySummary::default()
    };

    for entry in pane
        .entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, &pane.search, &chooser_patterns, &query))
    {
        summary.count += 1;
        if entry.is_dir {
            summary.folders += 1;
        } else {
            summary.files += 1;
        }
        summary.has_locations |= !entry.location.is_empty();
        if let Some(paths) = summary.visible_paths.as_mut() {
            paths.push(entry.path.clone());
        }
    }

    summary
}

#[cfg(test)]
pub(crate) fn filtered_entries_range(state: &AppState, range: Range<usize>) -> Vec<FileEntry> {
    filtered_entries_range_for_slot(state, 0, range)
}

#[allow(dead_code)]
pub(crate) fn filtered_entries_range_for_slot(
    state: &AppState,
    slot: i32,
    range: Range<usize>,
) -> Vec<FileEntry> {
    if range.is_empty() {
        return Vec::new();
    }

    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return Vec::new();
    };
    let chooser_patterns = active_chooser_patterns(state);
    let mut entries: Vec<FileEntry> = if let Some(indices) =
        pane.search.visible_entry_indices.as_ref()
    {
        let end = range.end.min(indices.len());
        indices
            .get(range.start..end)
            .unwrap_or(&[])
            .iter()
            .map(|&index| pane.entries[index].to_file_entry())
            .collect()
    } else if filters_are_identity(&pane.search, &chooser_patterns) {
        pane.entries
            .get(range.start..range.end.min(pane.entries.len()))
            .unwrap_or(&[])
            .iter()
            .map(PaneEntrySnapshot::to_file_entry)
            .collect()
    } else {
        let query = pane.search.query.to_ascii_lowercase();
        pane.entries
            .iter()
            .filter(|entry| matches_entry_filters(entry, &pane.search, &chooser_patterns, &query))
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
            .map(PaneEntrySnapshot::to_file_entry)
            .collect()
    };

    annotate_visible_location_groups_for_pane(state, pane, range.start, &mut entries);
    entries
}

#[cfg(test)]
#[allow(dead_code)]
fn annotate_visible_location_groups(
    state: &AppState,
    start_visible_index: usize,
    entries: &mut [FileEntry],
) {
    if let Some(pane) = state.panes.pane_for_slot(0) {
        annotate_visible_location_groups_for_pane(state, pane, start_visible_index, entries);
    }
}

#[allow(dead_code)]
fn annotate_visible_location_groups_for_pane(
    state: &AppState,
    pane: &PaneState,
    start_visible_index: usize,
    entries: &mut [FileEntry],
) {
    if !pane.search.visible_entries_have_locations {
        return;
    }

    if let Some(groups) = pane.search.visible_location_groups.as_ref() {
        for (offset, entry) in entries.iter_mut().enumerate() {
            entry.group = groups
                .get(start_visible_index + offset)
                .map_or_else(String::new, Clone::clone)
                .into();
        }
        return;
    }

    let mut previous_location = start_visible_index
        .checked_sub(1)
        .and_then(|index| visible_entry_location_at_for_pane(state, pane, index));
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
#[allow(dead_code)]
fn visible_entry_location_at(state: &AppState, visible_index: usize) -> Option<String> {
    state
        .panes
        .pane_for_slot(0)
        .and_then(|pane| visible_entry_location_at_for_pane(state, pane, visible_index))
}

#[allow(dead_code)]
fn visible_entry_location_at_for_pane(
    state: &AppState,
    pane: &PaneState,
    visible_index: usize,
) -> Option<String> {
    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        return indices
            .get(visible_index)
            .map(|&entry_index| pane.entries[entry_index].location.clone());
    }

    let chooser_patterns = active_chooser_patterns(state);
    if filters_are_identity(&pane.search, &chooser_patterns) {
        return pane
            .entries
            .get(visible_index)
            .map(|entry| entry.location.clone());
    }

    let query = pane.search.query.to_ascii_lowercase();
    pane.entries
        .iter()
        .filter(|entry| matches_entry_filters(entry, &pane.search, &chooser_patterns, &query))
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

fn location_group_labels<'a>(locations: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut previous_location: Option<&str> = None;
    locations
        .into_iter()
        .map(|location| {
            let group = if previous_location != Some(location) {
                search_group_label(location)
            } else {
                String::new()
            };
            previous_location = Some(location);
            group
        })
        .collect()
}

fn filters_are_identity(search: &PaneSearch, chooser_patterns: &[String]) -> bool {
    search.query.is_empty()
        && search.kind_filter == 0
        && search.modified_filter == 0
        && search.size_filter == 0
        && chooser_patterns.is_empty()
}

fn active_chooser_patterns(state: &AppState) -> Vec<String> {
    state
        .chooser_filters
        .get(state.chooser_filter_index)
        .map(|filter| filter.patterns.clone())
        .unwrap_or_default()
}

fn matches_entry_filters(
    entry: &PaneEntrySnapshot,
    search: &PaneSearch,
    chooser_patterns: &[String],
    query: &str,
) -> bool {
    matches_search_query(entry, query)
        && matches_kind_filter(entry, search.kind_filter)
        && matches_modified_filter(entry, search.modified_filter)
        && matches_size_filter(entry, search.size_filter)
        && matches_chooser_patterns(entry, chooser_patterns)
}

fn matches_search_query(entry: &PaneEntrySnapshot, query: &str) -> bool {
    query.is_empty()
        || entry.name.to_ascii_lowercase().contains(query)
        || entry.path.to_ascii_lowercase().contains(query)
}

fn matches_kind_filter(entry: &PaneEntrySnapshot, filter: i32) -> bool {
    match filter {
        1 => entry.is_dir,
        2 => !entry.is_dir,
        3 => !entry.is_dir && is_image_path(entry.path.as_str()),
        _ => true,
    }
}

fn matches_modified_filter(entry: &PaneEntrySnapshot, filter: i32) -> bool {
    match filter {
        1 => entry.modified_age_days == 0,
        2 => entry.modified_age_days >= 0 && entry.modified_age_days <= 7,
        3 => entry.modified_age_days >= 0 && entry.modified_age_days <= 30,
        _ => true,
    }
}

fn matches_size_filter(entry: &PaneEntrySnapshot, filter: i32) -> bool {
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

#[allow(dead_code)]
pub(crate) fn matches_chooser_filter(entry: &FileEntry, state: &AppState) -> bool {
    matches_chooser_patterns(
        &PaneEntrySnapshot::from_entry(entry),
        &active_chooser_patterns(state),
    )
}

fn matches_chooser_patterns(entry: &PaneEntrySnapshot, patterns: &[String]) -> bool {
    if entry.is_dir || patterns.is_empty() {
        return true;
    }

    patterns
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

#[cfg(test)]
pub(crate) fn selection_range_paths_filtered(
    state: &AppState,
    anchor: &str,
    target: &str,
) -> Vec<String> {
    selection_range_paths_filtered_for_slot(state, 0, anchor, target)
}

pub(crate) fn selection_range_paths_filtered_for_slot(
    state: &AppState,
    slot: i32,
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

    for entry in visible_entry_iter_for_slot(state, slot) {
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

#[cfg(test)]
pub(crate) fn selection_rect_paths(entries: &[FileEntry], rect: SelectionRect) -> Vec<String> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if rect.intersects_index(index) {
                Some(entry.path.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn selection_rect_paths_filtered(state: &AppState, rect: SelectionRect) -> Vec<String> {
    selection_rect_paths_filtered_for_slot(state, 0, rect)
}

pub(crate) fn selection_rect_paths_filtered_for_slot(
    state: &AppState,
    slot: i32,
    rect: SelectionRect,
) -> Vec<String> {
    let mut selected = Vec::new();
    let visible_range = selection_rect_visible_range_for_slot(state, slot, &rect);

    for (visible_index, entry) in visible_entries_range_iter_for_slot(state, slot, visible_range) {
        if rect.intersects_index(visible_index) {
            selected.push(entry.path.to_string());
        }
    }

    selected
}

#[allow(dead_code)]
fn selection_rect_visible_range(state: &AppState, rect: SelectionRect) -> Range<usize> {
    selection_rect_visible_range_for_slot(state, 0, &rect)
}

fn selection_rect_visible_range_for_slot(
    state: &AppState,
    slot: i32,
    rect: &SelectionRect,
) -> Range<usize> {
    let visible_count = filtered_entry_count_for_slot(state, slot);
    rect.candidate_range(visible_count)
}

#[allow(dead_code)]
fn visible_entries_range_iter(
    state: &AppState,
    range: Range<usize>,
) -> Box<dyn Iterator<Item = (usize, &PaneEntrySnapshot)> + '_> {
    visible_entries_range_iter_for_slot(state, 0, range)
}

fn visible_entries_range_iter_for_slot(
    state: &AppState,
    slot: i32,
    range: Range<usize>,
) -> Box<dyn Iterator<Item = (usize, &PaneEntrySnapshot)> + '_> {
    if range.is_empty() {
        return Box::new(std::iter::empty());
    }

    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return Box::new(std::iter::empty());
    };

    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        let start = range.start.min(indices.len());
        let end = range.end.min(indices.len());
        return Box::new(
            indices[start..end]
                .iter()
                .enumerate()
                .map(move |(offset, &index)| (start + offset, &pane.entries[index])),
        );
    }

    let chooser_patterns = active_chooser_patterns(state);
    if filters_are_identity(&pane.search, &chooser_patterns) {
        let start = range.start.min(pane.entries.len());
        let end = range.end.min(pane.entries.len());
        return Box::new(
            pane.entries[start..end]
                .iter()
                .enumerate()
                .map(move |(offset, entry)| (start + offset, entry)),
        );
    }

    let query = pane.search.query.to_ascii_lowercase();
    Box::new(
        pane.entries
            .iter()
            .filter(move |entry| {
                matches_entry_filters(entry, &pane.search, &chooser_patterns, &query)
            })
            .enumerate()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start)),
    )
}

#[allow(dead_code)]
fn visible_entry_iter(state: &AppState) -> Box<dyn Iterator<Item = &PaneEntrySnapshot> + '_> {
    visible_entry_iter_for_slot(state, 0)
}

fn visible_entry_iter_for_slot(
    state: &AppState,
    slot: i32,
) -> Box<dyn Iterator<Item = &PaneEntrySnapshot> + '_> {
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return Box::new(std::iter::empty());
    };

    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        return Box::new(indices.iter().map(|&index| &pane.entries[index]));
    }

    let chooser_patterns = active_chooser_patterns(state);
    if filters_are_identity(&pane.search, &chooser_patterns) {
        return Box::new(pane.entries.iter());
    }

    let query = pane.search.query.to_ascii_lowercase();
    Box::new(
        pane.entries.iter().filter(move |entry| {
            matches_entry_filters(entry, &pane.search, &chooser_patterns, &query)
        }),
    )
}

pub(crate) fn append_unique_paths(selected_paths: &mut Vec<String>, paths: Vec<String>) {
    for path in paths {
        if !selected_paths.iter().any(|selected| selected == &path) {
            selected_paths.push(path);
        }
    }
}
