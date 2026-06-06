use crate::app::geometry::ItemViewItemBounds;
use crate::app::item_view_renderer::ItemViewTileFrameSource;
use crate::app::pane::PaneView;
use crate::{
    ItemViewEntry, ItemViewFallbackMediaEntry, ItemViewHighlightEntry, ItemViewMediaEntry,
    ItemViewMetadataEntry, ItemViewPaintEntry,
};
use slint::{Image, Model, ModelRc, SharedString, VecModel};
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewRowToken {
    name: SharedString,
    path: SharedString,
    is_dir: bool,
    selected: bool,
    thumbnail_state: i32,
    media_token: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewMediaToken {
    slice_index: i32,
    media_token: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewMediaSource {
    pub(crate) slice_index: i32,
    pub(crate) media: Image,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewMetadataOverlaySource {
    pub(crate) slice_index: i32,
    pub(crate) text: SharedString,
    pub(crate) item_x: f32,
    pub(crate) item_y: f32,
    pub(crate) text_x: f32,
    pub(crate) text_width: f32,
    pub(crate) y: f32,
    pub(crate) line_height: f32,
    pub(crate) font_size: f32,
    pub(crate) is_group: bool,
}

impl ItemViewRowToken {
    pub(crate) fn from_entry(entry: &ItemViewEntry) -> Self {
        Self {
            name: entry.name.clone(),
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            selected: false,
            thumbnail_state: entry.thumbnail_state,
            media_token: entry.media_token,
        }
    }

    pub(crate) fn path(&self) -> &str {
        self.path.as_str()
    }

    pub(crate) fn path_shared(&self) -> SharedString {
        self.path.clone()
    }

    pub(crate) fn name_shared(&self) -> SharedString {
        self.name.clone()
    }

    pub(crate) fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub(crate) fn thumbnail_state(&self) -> i32 {
        self.thumbnail_state
    }

    pub(crate) fn media_token(&self) -> i32 {
        self.media_token
    }

    pub(crate) fn selected(&self) -> bool {
        self.selected
    }

    pub(crate) fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub(crate) fn row_equals_ignoring_selection(&self, other: &Self) -> bool {
        let mut current = self.clone();
        let mut next = other.clone();
        current.selected = false;
        next.selected = false;
        current == next
    }

    pub(crate) fn has_renderable_title(&self) -> bool {
        !self.name.as_str().trim().is_empty()
    }
}

pub(crate) fn new_item_view_entries_model(entries: Vec<ItemViewEntry>) -> ModelRc<ItemViewEntry> {
    ModelRc::new(Rc::new(VecModel::from(entries)))
}

pub(crate) fn new_item_view_bounds_model(
    bounds_entries: Vec<ItemViewItemBounds>,
) -> ModelRc<ItemViewItemBounds> {
    if bounds_entries.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(bounds_entries)))
}

fn update_item_view_bounds_entries_model(
    current: &mut ModelRc<ItemViewItemBounds>,
    old_start: usize,
    new_start: usize,
    bounds_entries: Vec<ItemViewItemBounds>,
) -> bool {
    if bounds_entries.is_empty() {
        if current.row_count() == 0 {
            return false;
        }
        *current = ModelRc::default();
        return true;
    }

    let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewItemBounds>>()
    else {
        *current = new_item_view_bounds_model(bounds_entries);
        return true;
    };

    update_sliding_vec_model(model, old_start, new_start, bounds_entries)
}

pub(crate) fn new_item_view_paint_model(
    paint_entries: Vec<ItemViewPaintEntry>,
) -> ModelRc<ItemViewPaintEntry> {
    if paint_entries.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(paint_entries)))
}

pub(crate) fn new_item_view_fallback_media_model(
    fallback_entries: Vec<ItemViewFallbackMediaEntry>,
) -> ModelRc<ItemViewFallbackMediaEntry> {
    if fallback_entries.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(fallback_entries)))
}

fn item_view_tile_frame_sources(
    entries: &[ItemViewEntry],
    bounds_entries: &[ItemViewItemBounds],
    selected_paths: &[String],
) -> Vec<ItemViewTileFrameSource> {
    let selected = selected_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();

    if bounds_entries.is_empty() {
        return entries
            .iter()
            .enumerate()
            .map(|(slice_index, entry)| {
                ItemViewTileFrameSource::from_entry_without_bounds(
                    slice_index,
                    entry,
                    selected.contains(entry.path.as_str()),
                )
            })
            .collect();
    }

    bounds_entries
        .iter()
        .filter_map(|bounds| {
            let entry = entries.get(bounds.slice_index)?;
            Some(ItemViewTileFrameSource::from_entry_and_bounds(
                entry,
                bounds,
                selected.contains(entry.path.as_str()),
            ))
        })
        .collect()
}

fn item_view_tile_frame_sources_from_tokens(
    tokens: &[ItemViewRowToken],
    bounds_entries: &[ItemViewItemBounds],
) -> Vec<ItemViewTileFrameSource> {
    bounds_entries
        .iter()
        .filter_map(|bounds| {
            let token = tokens.get(bounds.slice_index)?;
            Some(ItemViewTileFrameSource {
                slice_index: bounds.slice_index,
                name: token.name_shared(),
                is_dir: token.is_dir(),
                selected: token.selected(),
                media_token: token.media_token(),
                has_bounds: true,
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                text_width: bounds.text_width,
            })
        })
        .collect()
}

fn item_view_paint_entries(frames: &[ItemViewTileFrameSource]) -> Vec<ItemViewPaintEntry> {
    frames
        .iter()
        .filter(|frame| frame.has_bounds)
        .map(|frame| ItemViewPaintEntry {
            name: frame.name.clone(),
            x: frame.x,
            y: frame.y,
            width: frame.width,
            text_width: frame.text_width,
        })
        .collect()
}

fn update_item_view_paint_entries_model(
    current: &mut ModelRc<ItemViewPaintEntry>,
    old_start: usize,
    new_start: usize,
    paint_entries: Vec<ItemViewPaintEntry>,
) -> bool {
    if paint_entries.is_empty() {
        if current.row_count() == 0 {
            return false;
        }
        *current = ModelRc::default();
        return true;
    }

    let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewPaintEntry>>()
    else {
        *current = new_item_view_paint_model(paint_entries);
        return true;
    };

    update_sliding_vec_model(model, old_start, new_start, paint_entries)
}

fn item_view_fallback_media_entries(
    frames: &[ItemViewTileFrameSource],
    is_dir: bool,
) -> Vec<ItemViewFallbackMediaEntry> {
    frames
        .iter()
        .filter(|frame| frame.has_bounds)
        .filter_map(|frame| {
            (frame.is_dir == is_dir).then_some(ItemViewFallbackMediaEntry {
                x: frame.x,
                y: frame.y,
            })
        })
        .collect()
}

fn update_item_view_fallback_media_entries_model(
    current: &mut ModelRc<ItemViewFallbackMediaEntry>,
    fallback_entries: Vec<ItemViewFallbackMediaEntry>,
) -> bool {
    if fallback_entries.is_empty() {
        if current.row_count() == 0 {
            return false;
        }
        *current = ModelRc::default();
        return true;
    }

    let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewFallbackMediaEntry>>()
    else {
        *current = new_item_view_fallback_media_model(fallback_entries);
        return true;
    };

    update_sparse_vec_model(model, fallback_entries)
}

pub(crate) fn new_item_view_metadata_model(
    metadata: Vec<ItemViewMetadataEntry>,
) -> ModelRc<ItemViewMetadataEntry> {
    if metadata.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(metadata)))
}

fn update_item_view_metadata_entries_model(
    current: &mut ModelRc<ItemViewMetadataEntry>,
    metadata: Vec<ItemViewMetadataEntry>,
) -> bool {
    if metadata.is_empty() {
        if current.row_count() == 0 {
            return false;
        }
        *current = ModelRc::default();
        return true;
    }

    let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewMetadataEntry>>()
    else {
        *current = new_item_view_metadata_model(metadata);
        return true;
    };

    update_sparse_vec_model(model, metadata)
}

fn bounds_for_slice_index(
    bounds_entries: &[ItemViewItemBounds],
    slice_index: i32,
) -> Option<&ItemViewItemBounds> {
    let slice_index = usize::try_from(slice_index).ok()?;
    bounds_entries
        .get(slice_index)
        .filter(|bounds| bounds.slice_index == slice_index)
        .or_else(|| {
            bounds_entries
                .iter()
                .find(|bounds| bounds.slice_index == slice_index)
        })
}

fn project_metadata_entries_with_bounds(
    metadata: Vec<ItemViewMetadataOverlaySource>,
    bounds_entries: &[ItemViewItemBounds],
) -> Vec<ItemViewMetadataEntry> {
    metadata
        .into_iter()
        .map(|metadata| {
            let bounds = bounds_for_slice_index(bounds_entries, metadata.slice_index);
            ItemViewMetadataEntry {
                text: metadata.text,
                item_x: bounds.map_or(metadata.item_x, |b| b.x),
                item_y: bounds.map_or(metadata.item_y, |b| b.y),
                text_x: metadata.text_x,
                text_width: metadata.text_width,
                y: metadata.y,
                line_height: metadata.line_height,
                font_size: metadata.font_size,
                is_group: metadata.is_group,
            }
        })
        .collect()
}

pub(crate) fn new_item_view_media_model(
    media_entries: Vec<ItemViewMediaEntry>,
) -> ModelRc<ItemViewMediaEntry> {
    if media_entries.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(media_entries)))
}

fn project_media_entries_with_bounds(
    media_entries: Vec<ItemViewMediaSource>,
    bounds_entries: &[ItemViewItemBounds],
) -> Vec<ItemViewMediaEntry> {
    media_entries
        .into_iter()
        .map(|media| {
            let bounds = bounds_for_slice_index(bounds_entries, media.slice_index);
            ItemViewMediaEntry {
                media: media.media,
                x: bounds.map_or(media.x, |b| b.x),
                y: bounds.map_or(media.y, |b| b.y),
            }
        })
        .collect()
}

fn item_view_media_tokens(
    frames: &[ItemViewTileFrameSource],
    media_entries: &[ItemViewMediaSource],
) -> Vec<ItemViewMediaToken> {
    media_entries
        .iter()
        .map(|media| {
            let media_token = usize::try_from(media.slice_index)
                .ok()
                .and_then(|row| frames.get(row))
                .map_or(0, |frame| frame.media_token);
            ItemViewMediaToken {
                slice_index: media.slice_index,
                media_token,
            }
        })
        .collect()
}

fn update_item_view_media_entries_model(
    current: &mut ModelRc<ItemViewMediaEntry>,
    current_tokens: &mut Vec<ItemViewMediaToken>,
    media_entries: Vec<ItemViewMediaEntry>,
    next_tokens: Vec<ItemViewMediaToken>,
) -> bool {
    if media_entries.is_empty() {
        let had_tokens = !current_tokens.is_empty();
        current_tokens.clear();
        if current.row_count() == 0 {
            return had_tokens;
        }
        *current = ModelRc::default();
        return true;
    }

    let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewMediaEntry>>()
    else {
        *current = new_item_view_media_model(media_entries);
        *current_tokens = next_tokens;
        return true;
    };

    let changed =
        update_sparse_vec_model_by_tokens(model, current_tokens, media_entries, next_tokens);
    changed
}

fn item_view_highlight_entries(frames: &[ItemViewTileFrameSource]) -> Vec<ItemViewHighlightEntry> {
    frames
        .iter()
        .filter(|frame| frame.has_bounds)
        .filter_map(|frame| {
            frame.selected.then_some(ItemViewHighlightEntry {
                x: frame.x,
                y: frame.y,
                width: frame.width,
            })
        })
        .collect()
}

fn current_item_view_bounds_entries(view: &PaneView) -> Vec<ItemViewItemBounds> {
    (0..view.virtual_bounds_entries.row_count())
        .filter_map(|row| view.virtual_bounds_entries.row_data(row))
        .collect()
}

fn update_item_view_highlight_entries_model(
    current: &mut ModelRc<ItemViewHighlightEntry>,
    highlights: Vec<ItemViewHighlightEntry>,
) -> bool {
    let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewHighlightEntry>>()
    else {
        if highlights.is_empty() {
            if current.row_count() == 0 {
                return false;
            }
            *current = ModelRc::default();
        } else {
            *current = ModelRc::new(Rc::new(VecModel::from(highlights)));
        }
        return true;
    };

    let unchanged = model.row_count() == highlights.len()
        && highlights
            .iter()
            .enumerate()
            .all(|(row, next)| model.row_data(row).as_ref() == Some(next));
    if unchanged {
        return false;
    }

    model.set_vec(highlights);
    true
}

pub(crate) fn update_item_view_highlight_model(view: &mut PaneView) -> bool {
    let bounds_entries = current_item_view_bounds_entries(view);
    let frames =
        item_view_tile_frame_sources_from_tokens(&view.virtual_entry_tokens, &bounds_entries);
    update_item_view_highlight_entries_model(
        &mut view.virtual_highlight_entries,
        item_view_highlight_entries(&frames),
    )
}

fn item_view_row_tokens(
    entries: &[ItemViewEntry],
    selected_paths: &[String],
) -> Vec<ItemViewRowToken> {
    let selected = selected_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    entries
        .iter()
        .map(|entry| {
            let mut token = ItemViewRowToken::from_entry(entry);
            token.set_selected(selected.contains(entry.path.as_str()));
            token
        })
        .collect()
}

pub(crate) fn update_item_view_entries_model(
    current: &ModelRc<ItemViewEntry>,
    old_start: usize,
    new_start: usize,
    current_tokens: &mut Vec<ItemViewRowToken>,
    entries: Vec<ItemViewEntry>,
    selected_paths: &[String],
) -> Option<ModelRc<ItemViewEntry>> {
    let mut next_tokens = item_view_row_tokens(&entries, selected_paths);
    let Some(model) = current.as_any().downcast_ref::<VecModel<ItemViewEntry>>() else {
        *current_tokens = next_tokens;
        return Some(new_item_view_entries_model(entries));
    };

    update_vec_model(
        model,
        old_start,
        new_start,
        current_tokens,
        &mut next_tokens,
        entries,
    );
    None
}

pub(crate) fn update_pane_item_view_entries_model(
    view: &mut PaneView,
    start_index: usize,
    entries: Vec<ItemViewEntry>,
    bounds_entries: Vec<ItemViewItemBounds>,
    media_entries: Vec<ItemViewMediaSource>,
    metadata_entries: Vec<ItemViewMetadataOverlaySource>,
    selected_paths: &[String],
) {
    let old_start = view.virtual_start_index;
    let frame_sources = item_view_tile_frame_sources(&entries, &bounds_entries, selected_paths);
    let media_tokens = item_view_media_tokens(&frame_sources, &media_entries);
    let media_entries = project_media_entries_with_bounds(media_entries, &bounds_entries);
    let metadata_entries = project_metadata_entries_with_bounds(metadata_entries, &bounds_entries);
    let paint_entries = item_view_paint_entries(&frame_sources);
    let folder_media_entries = item_view_fallback_media_entries(&frame_sources, true);
    let file_media_entries = item_view_fallback_media_entries(&frame_sources, false);
    update_item_view_bounds_entries_model(
        &mut view.virtual_bounds_entries,
        old_start,
        start_index,
        bounds_entries,
    );
    update_item_view_paint_entries_model(
        &mut view.virtual_paint_entries,
        old_start,
        start_index,
        paint_entries,
    );
    update_item_view_fallback_media_entries_model(
        &mut view.virtual_folder_media_entries,
        folder_media_entries,
    );
    update_item_view_fallback_media_entries_model(
        &mut view.virtual_file_media_entries,
        file_media_entries,
    );
    update_item_view_media_entries_model(
        &mut view.virtual_media_entries,
        &mut view.virtual_media_tokens,
        media_entries,
        media_tokens,
    );
    update_item_view_metadata_entries_model(&mut view.virtual_metadata_entries, metadata_entries);
    let current = view.virtual_entries.clone();
    if let Some(model) = update_item_view_entries_model(
        &current,
        old_start,
        start_index,
        &mut view.virtual_entry_tokens,
        entries,
        selected_paths,
    ) {
        view.virtual_entries = model;
    }
    update_item_view_highlight_model(view);
    view.virtual_start_index = start_index;
}

pub(crate) fn relayout_pane_item_view_entries_model(
    view: &mut PaneView,
    range: Range<usize>,
    bounds_entries: Vec<ItemViewItemBounds>,
) -> bool {
    let Some(model) = view
        .virtual_entries
        .as_any()
        .downcast_ref::<VecModel<ItemViewEntry>>()
    else {
        return false;
    };
    let row_count = model.row_count();
    if row_count != view.virtual_entry_tokens.len() {
        return false;
    }
    let old_start = view.virtual_start_index;
    let old_end = old_start.saturating_add(row_count);
    if range.is_empty() || range.start < old_start || range.end > old_end || range.end < range.start
    {
        return false;
    }

    let remove_front = range.start - old_start;
    for _ in 0..remove_front {
        model.remove(0);
    }
    view.virtual_entry_tokens
        .drain(0..remove_front.min(view.virtual_entry_tokens.len()));

    let target_len = range.end - range.start;
    while model.row_count() > target_len {
        model.remove(model.row_count() - 1);
        view.virtual_entry_tokens.pop();
    }
    if model.row_count() != target_len || view.virtual_entry_tokens.len() != target_len {
        return false;
    }

    let frame_sources =
        item_view_tile_frame_sources_from_tokens(&view.virtual_entry_tokens, &bounds_entries);
    let paint_entries = item_view_paint_entries(&frame_sources);
    let folder_media_entries = item_view_fallback_media_entries(&frame_sources, true);
    let file_media_entries = item_view_fallback_media_entries(&frame_sources, false);
    let highlight_entries = item_view_highlight_entries(&frame_sources);
    trim_item_view_media_entries_model(
        &mut view.virtual_media_entries,
        &mut view.virtual_media_tokens,
        remove_front,
        target_len,
        &bounds_entries,
    );
    update_item_view_bounds_entries_model(
        &mut view.virtual_bounds_entries,
        old_start,
        range.start,
        bounds_entries,
    );
    update_item_view_paint_entries_model(
        &mut view.virtual_paint_entries,
        old_start,
        range.start,
        paint_entries,
    );
    update_item_view_fallback_media_entries_model(
        &mut view.virtual_folder_media_entries,
        folder_media_entries,
    );
    update_item_view_fallback_media_entries_model(
        &mut view.virtual_file_media_entries,
        file_media_entries,
    );
    let _ = update_item_view_highlight_entries_model(
        &mut view.virtual_highlight_entries,
        highlight_entries,
    );
    view.virtual_start_index = range.start;
    true
}

fn trim_item_view_media_entries_model(
    current: &mut ModelRc<ItemViewMediaEntry>,
    current_tokens: &mut Vec<ItemViewMediaToken>,
    remove_front: usize,
    target_len: usize,
    bounds_entries: &[ItemViewItemBounds],
) {
    let retained = (0..current.row_count())
        .filter_map(|row| {
            current.row_data(row).map(|entry| {
                let token = current_tokens
                    .get(row)
                    .cloned()
                    .unwrap_or(ItemViewMediaToken {
                        slice_index: row as i32,
                        media_token: 0,
                    });
                (entry, token)
            })
        })
        .filter_map(|(mut entry, mut token)| {
            let slice_index = usize::try_from(token.slice_index).ok()?;
            if slice_index < remove_front {
                return None;
            }
            let shifted = slice_index - remove_front;
            if shifted >= target_len {
                return None;
            }
            token.slice_index = shifted as i32;
            if let Some(bounds) = bounds_for_slice_index(bounds_entries, token.slice_index) {
                entry.x = bounds.x;
                entry.y = bounds.y;
            }
            Some((entry, token))
        })
        .collect::<Vec<_>>();

    let (entries, tokens): (Vec<_>, Vec<_>) = retained.into_iter().unzip();

    if let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<ItemViewMediaEntry>>()
    {
        model.set_vec(entries);
    } else {
        *current = new_item_view_media_model(entries);
    }
    *current_tokens = tokens;
}

pub(crate) fn update_pane_item_view_selection_model(
    view: &mut PaneView,
    selected_paths: &[String],
) -> bool {
    let changed = update_item_view_selection_tokens(&mut view.virtual_entry_tokens, selected_paths);
    if changed {
        update_item_view_highlight_model(view);
    }
    changed
}

pub(crate) fn update_item_view_selection_tokens(
    current_tokens: &mut [ItemViewRowToken],
    selected_paths: &[String],
) -> bool {
    if selected_paths.is_empty() {
        let mut changed = false;
        for token in current_tokens {
            if token.selected() {
                token.set_selected(false);
                changed = true;
            }
        }
        return changed;
    }

    let selected = selected_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let mut changed = false;
    for token in current_tokens.iter_mut() {
        let selected = selected.contains(token.path());
        if token.selected() != selected {
            token.set_selected(selected);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
fn selected_token_rows(current_tokens: &[ItemViewRowToken]) -> Vec<String> {
    current_tokens
        .iter()
        .filter(|token| token.selected())
        .map(|token| token.path().to_string())
        .collect()
}

fn update_vec_model(
    model: &VecModel<ItemViewEntry>,
    old_start: usize,
    new_start: usize,
    current_tokens: &mut Vec<ItemViewRowToken>,
    next_tokens: &mut Vec<ItemViewRowToken>,
    entries: Vec<ItemViewEntry>,
) {
    let old_len = model.row_count();
    if old_len == 0 || entries.is_empty() {
        model.set_vec(entries);
        *current_tokens = std::mem::take(next_tokens);
        return;
    }

    let old_end = old_start.saturating_add(old_len);
    let new_end = new_start.saturating_add(entries.len());
    let overlap_start = old_start.max(new_start);
    let overlap_end = old_end.min(new_end);

    if overlap_start >= overlap_end {
        model.set_vec(entries);
        *current_tokens = std::mem::take(next_tokens);
        return;
    }

    if new_start > old_start {
        let remove_count = (new_start - old_start).min(model.row_count());
        for _ in 0..remove_count {
            model.remove(0);
        }
        current_tokens.drain(0..remove_count.min(current_tokens.len()));
    } else if new_start < old_start {
        let prefix_len = (old_start - new_start).min(entries.len());
        for entry in entries[..prefix_len].iter().rev() {
            model.insert(0, entry.clone());
        }
        current_tokens.splice(0..0, next_tokens[..prefix_len].iter().cloned());
    }

    let overlap_rows = overlap_start - new_start..overlap_end - new_start;
    for row in overlap_rows {
        let rows_differ = current_tokens
            .get(row)
            .zip(next_tokens.get(row))
            .is_none_or(|(current, next)| !current.row_equals_ignoring_selection(next));
        if rows_differ {
            model.set_row_data(row, entries[row].clone());
        }
    }

    while model.row_count() > entries.len() {
        model.remove(model.row_count() - 1);
    }

    let current_len = model.row_count();
    if current_len < entries.len() {
        model.extend(entries[current_len..].iter().cloned());
    }
    *current_tokens = std::mem::take(next_tokens);
}

fn update_sliding_vec_model<T>(
    model: &VecModel<T>,
    old_start: usize,
    new_start: usize,
    entries: Vec<T>,
) -> bool
where
    T: Clone + PartialEq + 'static,
{
    let old_len = model.row_count();
    if old_len == 0 || entries.is_empty() {
        let changed = old_len != entries.len()
            || entries
                .iter()
                .enumerate()
                .any(|(row, entry)| model.row_data(row).as_ref() != Some(entry));
        if changed {
            model.set_vec(entries);
        }
        return changed;
    }

    let old_end = old_start.saturating_add(old_len);
    let new_end = new_start.saturating_add(entries.len());
    let overlap_start = old_start.max(new_start);
    let overlap_end = old_end.min(new_end);

    if overlap_start >= overlap_end {
        let changed = old_len != entries.len()
            || entries
                .iter()
                .enumerate()
                .any(|(row, entry)| model.row_data(row).as_ref() != Some(entry));
        if changed {
            model.set_vec(entries);
        }
        return changed;
    }

    let mut changed = false;
    if new_start > old_start {
        let remove_count = (new_start - old_start).min(model.row_count());
        for _ in 0..remove_count {
            model.remove(0);
            changed = true;
        }
    } else if new_start < old_start {
        let prefix_len = (old_start - new_start).min(entries.len());
        for entry in entries[..prefix_len].iter().rev() {
            model.insert(0, entry.clone());
            changed = true;
        }
    }

    let overlap_rows = overlap_start - new_start..overlap_end - new_start;
    for row in overlap_rows {
        if model.row_data(row).as_ref() != Some(&entries[row]) {
            model.set_row_data(row, entries[row].clone());
            changed = true;
        }
    }

    while model.row_count() > entries.len() {
        model.remove(model.row_count() - 1);
        changed = true;
    }

    let current_len = model.row_count();
    if current_len < entries.len() {
        model.extend(entries[current_len..].iter().cloned());
        changed = true;
    }
    changed
}

fn update_sparse_vec_model<T>(model: &VecModel<T>, entries: Vec<T>) -> bool
where
    T: Clone + PartialEq + 'static,
{
    let mut changed = false;
    let overlap_len = model.row_count().min(entries.len());
    for (row, entry) in entries.iter().enumerate().take(overlap_len) {
        if model.row_data(row).as_ref() != Some(entry) {
            model.set_row_data(row, entry.clone());
            changed = true;
        }
    }

    while model.row_count() > entries.len() {
        model.remove(model.row_count() - 1);
        changed = true;
    }

    let current_len = model.row_count();
    if current_len < entries.len() {
        model.extend(entries[current_len..].iter().cloned());
        changed = true;
    }

    changed
}

fn update_sparse_vec_model_by_tokens<T, U>(
    model: &VecModel<T>,
    current_tokens: &mut Vec<U>,
    entries: Vec<T>,
    next_tokens: Vec<U>,
) -> bool
where
    T: Clone + 'static,
    U: Clone + PartialEq,
{
    let mut changed = false;
    let overlap_len = model.row_count().min(entries.len());
    for row in 0..overlap_len {
        let row_changed = current_tokens
            .get(row)
            .zip(next_tokens.get(row))
            .is_none_or(|(current, next)| current != next);
        if row_changed {
            model.set_row_data(row, entries[row].clone());
            changed = true;
        }
    }

    while model.row_count() > entries.len() {
        model.remove(model.row_count() - 1);
        changed = true;
    }

    let current_len = model.row_count();
    if current_len < entries.len() {
        model.extend(entries[current_len..].iter().cloned());
        changed = true;
    }

    if *current_tokens != next_tokens {
        *current_tokens = next_tokens;
        changed = true;
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::pane::PaneView;
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

    fn entry(index: usize) -> ItemViewEntry {
        ItemViewEntry {
            name: format!("item-{index}").into(),
            path: format!("/tmp/item-{index}").into(),
            is_dir: false,
            thumbnail_state: 0,
            media_token: 0,
        }
    }

    fn rows(model: &ModelRc<ItemViewEntry>) -> Vec<String> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| entry.path.to_string())
            .collect()
    }

    fn highlight_rows(model: &ModelRc<ItemViewHighlightEntry>) -> Vec<(f32, f32, f32)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.x, entry.y, entry.width))
            .collect()
    }

    fn highlight_geometry_rows(model: &ModelRc<ItemViewHighlightEntry>) -> Vec<(f32, f32, f32)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.x, entry.y, entry.width))
            .collect()
    }

    fn bounds_row_x(model: &ModelRc<ItemViewItemBounds>) -> Vec<f32> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| entry.x)
            .collect()
    }

    fn paint_rows(model: &ModelRc<ItemViewPaintEntry>) -> Vec<(String, f32, f32)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.name.to_string(), entry.x, entry.text_width))
            .collect()
    }

    fn bounds_entries(start: usize, count: usize) -> Vec<ItemViewItemBounds> {
        (0..count)
            .map(|row| {
                let index = start + row;
                ItemViewItemBounds {
                    slice_index: row,
                    x: index as f32 * 10.0,
                    y: row as f32 * 2.0,
                    width: 90.0 + index as f32,
                    text_width: 45.0 + index as f32,
                }
            })
            .collect()
    }

    fn media_rows(model: &ModelRc<ItemViewMediaEntry>) -> Vec<Rgba8Pixel> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| first_pixel(&entry.media))
            .collect()
    }

    fn media_geometry_rows(model: &ModelRc<ItemViewMediaEntry>) -> Vec<(f32, f32)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.x, entry.y))
            .collect()
    }

    fn media_tokens(tokens: &[ItemViewMediaToken]) -> Vec<(i32, i32)> {
        tokens
            .iter()
            .map(|token| (token.slice_index, token.media_token))
            .collect()
    }

    fn fallback_rows(model: &ModelRc<ItemViewFallbackMediaEntry>) -> Vec<(f32, f32)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.x, entry.y))
            .collect()
    }

    fn metadata_rows(model: &ModelRc<ItemViewMetadataEntry>) -> Vec<(String, bool)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.text.to_string(), entry.is_group))
            .collect()
    }

    fn metadata_geometry_rows(model: &ModelRc<ItemViewMetadataEntry>) -> Vec<(f32, f32)> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| (entry.item_x, entry.item_y))
            .collect()
    }

    fn metadata_entries(start: usize, count: usize) -> Vec<ItemViewMetadataOverlaySource> {
        (0..count)
            .flat_map(|row| {
                let index = start + row;
                [
                    ItemViewMetadataOverlaySource {
                        slice_index: row as i32,
                        text: format!("Group {index}").into(),
                        item_x: 0.0,
                        item_y: 0.0,
                        text_x: 52.0,
                        text_width: 75.0 + index as f32,
                        y: 2.0,
                        line_height: 14.0,
                        font_size: 11.0,
                        is_group: true,
                    },
                    ItemViewMetadataOverlaySource {
                        slice_index: row as i32,
                        text: format!("/tmp/group-{index}").into(),
                        item_x: 0.0,
                        item_y: 0.0,
                        text_x: 52.0,
                        text_width: 75.0 + index as f32,
                        y: 41.0,
                        line_height: 14.0,
                        font_size: 11.0,
                        is_group: false,
                    },
                ]
            })
            .collect()
    }

    fn media_source(slice_index: i32, pixel: Rgba8Pixel) -> ItemViewMediaSource {
        ItemViewMediaSource {
            slice_index,
            media: colored_image(pixel),
            x: 0.0,
            y: 0.0,
        }
    }

    fn entries_with_tile_metrics(count: usize) -> Vec<ItemViewEntry> {
        (0..count).map(entry).collect()
    }

    fn colored_image(pixel: Rgba8Pixel) -> Image {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        buffer.make_mut_slice()[0] = pixel;
        Image::from_rgba8(buffer)
    }

    fn first_pixel(image: &Image) -> Rgba8Pixel {
        image
            .to_rgba8()
            .expect("test image should be rgba")
            .as_slice()[0]
    }

    #[test]
    fn sparse_overlay_models_carry_projected_item_bounds() {
        let mut entries = entries_with_tile_metrics(3);
        entries[1].thumbnail_state = 2;
        entries[1].media_token = 101;
        let bounds = bounds_entries(20, 3);
        let mut view = PaneView::default();

        update_pane_item_view_entries_model(
            &mut view,
            20,
            entries,
            bounds,
            vec![media_source(1, Rgba8Pixel::new(255, 0, 0, 255))],
            vec![ItemViewMetadataOverlaySource {
                slice_index: 1,
                text: "Group".into(),
                item_x: 0.0,
                item_y: 0.0,
                text_x: 52.0,
                text_width: 75.0,
                y: 2.0,
                line_height: 14.0,
                font_size: 11.0,
                is_group: true,
            }],
            &["/tmp/item-1".to_string()],
        );

        assert_eq!(
            highlight_geometry_rows(&view.virtual_highlight_entries),
            vec![(210.0, 2.0, 111.0)]
        );
        assert_eq!(
            media_geometry_rows(&view.virtual_media_entries),
            vec![(210.0, 2.0)]
        );
        assert_eq!(
            metadata_geometry_rows(&view.virtual_metadata_entries),
            vec![(210.0, 2.0)]
        );
    }

    #[test]
    fn tile_frame_sources_drive_split_primitive_models() {
        let mut entries = entries_with_tile_metrics(3);
        entries[0].is_dir = true;
        entries[1].media_token = 77;
        let bounds = bounds_entries(10, 3);

        let frames = item_view_tile_frame_sources(&entries, &bounds, &["/tmp/item-1".to_string()]);
        let paint = item_view_paint_entries(&frames);
        let folder_fallback = item_view_fallback_media_entries(&frames, true);
        let file_fallback = item_view_fallback_media_entries(&frames, false);
        let highlights = item_view_highlight_entries(&frames);
        let projected_media_tokens =
            item_view_media_tokens(&frames, &[media_source(1, Rgba8Pixel::new(255, 0, 0, 255))]);

        assert_eq!(frames.len(), 3);
        assert_eq!(paint[1].name, "item-1");
        assert_eq!(paint[1].x, 110.0);
        assert_eq!(paint[1].text_width, 56.0);
        assert_eq!(
            folder_fallback,
            vec![ItemViewFallbackMediaEntry { x: 100.0, y: 0.0 }]
        );
        assert_eq!(
            file_fallback,
            vec![
                ItemViewFallbackMediaEntry { x: 110.0, y: 2.0 },
                ItemViewFallbackMediaEntry { x: 120.0, y: 4.0 },
            ]
        );
        assert_eq!(
            highlights,
            vec![ItemViewHighlightEntry {
                x: 110.0,
                y: 2.0,
                width: 101.0
            }]
        );
        assert_eq!(media_tokens(&projected_media_tokens), vec![(1, 77)]);
    }

    #[test]
    fn pane_item_view_entry_model_updates_each_view_independently() {
        let mut left = PaneView::default();
        let mut right = PaneView::default();

        update_pane_item_view_entries_model(
            &mut left,
            0,
            (0..3).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );
        update_pane_item_view_entries_model(
            &mut right,
            20,
            (20..23).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(left.virtual_start_index, 0);
        assert_eq!(right.virtual_start_index, 20);
        assert_eq!(
            rows(&left.virtual_entries),
            (0..3)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            rows(&right.virtual_entries),
            (20..23)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );

        update_pane_item_view_entries_model(
            &mut right,
            22,
            (22..25).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(
            rows(&left.virtual_entries),
            (0..3)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(right.virtual_start_index, 22);
    }

    #[test]
    fn item_view_entry_model_reuses_vec_model_when_range_slides_forward() {
        let initial_entries = (0..6).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                2,
                &mut tokens,
                (2..8).map(entry).collect(),
                &[]
            )
            .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(tokens.len(), 6);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_reuses_vec_model_when_range_slides_backward() {
        let initial_entries = (4..10).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                4,
                2,
                &mut tokens,
                (2..8).map(entry).collect(),
                &[]
            )
            .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(tokens.len(), 6);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_resets_same_vec_model_without_overlap() {
        let initial_entries = (0..3).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                20,
                &mut tokens,
                (20..23).map(entry).collect(),
                &[]
            )
            .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(tokens.len(), 3);
        assert_eq!(
            rows(&model),
            (20..23)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_repairs_missing_sidecar_tokens_after_update() {
        let initial_entries = (0..6).map(entry).collect::<Vec<_>>();
        let mut tokens = Vec::new();
        let model = new_item_view_entries_model(initial_entries);

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                2,
                &mut tokens,
                (2..8).map(entry).collect(),
                &[]
            )
            .is_none()
        );

        assert_eq!(tokens.len(), 6);
        assert_eq!(
            rows(&model),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn item_view_entry_model_ignores_pane_level_layout_for_same_row() {
        let initial = entry(0);
        let mut tokens = item_view_row_tokens(&[initial.clone()], &[]);
        let model = new_item_view_entries_model(vec![initial]);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![entry(0)], &[])
                .is_none()
        );

        assert_eq!(model, original);
        let updated = model.row_data(0).expect("row should remain present");
        assert_eq!(updated.name, "item-0");
    }

    #[test]
    fn item_view_entry_model_uses_media_token_without_row_images() {
        let mut old_entry = entry(0);
        old_entry.media_token = 42;
        let initial_entries = vec![old_entry];
        let mut tokens = item_view_row_tokens(&initial_entries, &[]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        let mut same_token_entry = entry(0);
        same_token_entry.media_token = 42;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![same_token_entry], &[])
                .is_none()
        );

        assert_eq!(model, original);
        let unchanged = model.row_data(0).expect("row should remain present");
        assert_eq!(unchanged.media_token, 42);

        let mut new_token_entry = entry(0);
        new_token_entry.media_token = 43;
        assert!(
            update_item_view_entries_model(&model, 0, 0, &mut tokens, vec![new_token_entry], &[])
                .is_none()
        );

        let updated = model.row_data(0).expect("row should remain present");
        assert_eq!(updated.media_token, 43);
    }

    #[test]
    fn pane_item_view_entry_model_updates_selection_sidecar_without_replacing_entries() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(4),
            bounds_entries(0, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );
        let original = view.virtual_entries.clone();

        assert!(update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        assert_eq!(view.virtual_entries, original);
        assert_eq!(
            selected_token_rows(&view.virtual_entry_tokens),
            vec!["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        );
        assert_eq!(
            highlight_rows(&view.virtual_highlight_entries),
            vec![(10.0, 2.0, 91.0), (30.0, 6.0, 93.0)]
        );

        let selected_highlights = view.virtual_highlight_entries.clone();
        assert!(!update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        assert_eq!(view.virtual_highlight_entries, selected_highlights);

        assert!(update_pane_item_view_selection_model(&mut view, &[]));
        assert!(selected_token_rows(&view.virtual_entry_tokens).is_empty());
        assert_eq!(view.virtual_highlight_entries, selected_highlights);
        assert!(highlight_rows(&view.virtual_highlight_entries).is_empty());
    }

    #[test]
    fn pane_item_view_bounds_model_reuses_vec_model_when_range_slides() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(4),
            bounds_entries(0, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );
        let original_bounds = view.virtual_bounds_entries.clone();

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..6).map(entry).collect(),
            bounds_entries(2, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_bounds_entries, original_bounds);
        assert_eq!(
            bounds_row_x(&view.virtual_bounds_entries),
            vec![20.0, 30.0, 40.0, 50.0]
        );

        update_pane_item_view_entries_model(
            &mut view,
            1,
            (1..5).map(entry).collect(),
            bounds_entries(1, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_bounds_entries, original_bounds);
        assert_eq!(
            bounds_row_x(&view.virtual_bounds_entries),
            vec![10.0, 20.0, 30.0, 40.0]
        );
    }

    #[test]
    fn pane_item_view_paint_model_reuses_vec_model_when_range_slides() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(4),
            bounds_entries(0, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );
        let original_paint = view.virtual_paint_entries.clone();

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..6).map(entry).collect(),
            bounds_entries(2, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_paint_entries, original_paint);
        assert_eq!(
            paint_rows(&view.virtual_paint_entries),
            vec![
                ("item-2".to_string(), 20.0, 47.0),
                ("item-3".to_string(), 30.0, 48.0),
                ("item-4".to_string(), 40.0, 49.0),
                ("item-5".to_string(), 50.0, 50.0),
            ]
        );

        update_pane_item_view_entries_model(
            &mut view,
            1,
            (1..5).map(entry).collect(),
            bounds_entries(1, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_paint_entries, original_paint);
        assert_eq!(
            paint_rows(&view.virtual_paint_entries),
            vec![
                ("item-1".to_string(), 10.0, 46.0),
                ("item-2".to_string(), 20.0, 47.0),
                ("item-3".to_string(), 30.0, 48.0),
                ("item-4".to_string(), 40.0, 49.0),
            ]
        );
    }

    #[test]
    fn pane_item_view_metadata_model_reuses_vec_model_for_sparse_updates() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(3),
            Vec::new(),
            Vec::new(),
            metadata_entries(0, 3),
            &[],
        );
        let original_metadata = view.virtual_metadata_entries.clone();

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..5).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            metadata_entries(2, 3),
            &[],
        );

        assert_eq!(view.virtual_metadata_entries, original_metadata);
        assert_eq!(
            metadata_rows(&view.virtual_metadata_entries),
            vec![
                ("Group 2".to_string(), true),
                ("/tmp/group-2".to_string(), false),
                ("Group 3".to_string(), true),
                ("/tmp/group-3".to_string(), false),
                ("Group 4".to_string(), true),
                ("/tmp/group-4".to_string(), false),
            ]
        );

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..5).map(entry).collect(),
            Vec::new(),
            Vec::new(),
            metadata_entries(2, 3),
            &[],
        );

        assert_eq!(view.virtual_metadata_entries, original_metadata);
    }

    #[test]
    fn pane_item_view_metadata_model_clears_sparse_rows_without_stale_text() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(2),
            Vec::new(),
            Vec::new(),
            metadata_entries(0, 2),
            &[],
        );
        assert_eq!(view.virtual_metadata_entries.row_count(), 4);

        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(2),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_metadata_entries.row_count(), 0);
    }

    #[test]
    fn pane_item_view_media_model_reuses_vec_model_without_image_comparison() {
        let mut entries = entries_with_tile_metrics(4);
        entries[1].thumbnail_state = 2;
        entries[1].media_token = 101;
        entries[3].thumbnail_state = 2;
        entries[3].media_token = 103;
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries,
            Vec::new(),
            vec![
                media_source(1, Rgba8Pixel::new(255, 0, 0, 255)),
                media_source(3, Rgba8Pixel::new(0, 0, 255, 255)),
            ],
            Vec::new(),
            &[],
        );
        let original_media = view.virtual_media_entries.clone();

        let mut updated_entries = entries_with_tile_metrics(4);
        updated_entries[1].thumbnail_state = 2;
        updated_entries[1].media_token = 201;
        updated_entries[3].thumbnail_state = 2;
        updated_entries[3].media_token = 203;
        update_pane_item_view_entries_model(
            &mut view,
            0,
            updated_entries,
            Vec::new(),
            vec![
                media_source(1, Rgba8Pixel::new(0, 255, 0, 255)),
                media_source(3, Rgba8Pixel::new(255, 255, 0, 255)),
            ],
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_media_entries, original_media);
        assert_eq!(
            media_rows(&view.virtual_media_entries),
            vec![
                Rgba8Pixel::new(0, 255, 0, 255),
                Rgba8Pixel::new(255, 255, 0, 255),
            ]
        );
        assert_eq!(
            media_tokens(&view.virtual_media_tokens),
            vec![(1, 201), (3, 203)]
        );
    }

    #[test]
    fn fallback_media_model_uses_sparse_updates_not_continuous_range_sliding() {
        let source = include_str!("model_update.rs");
        let body = source
            .split_once("fn update_item_view_fallback_media_entries_model(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn new_item_view_metadata_model"))
            .map(|(body, _)| body)
            .expect("fallback media update helper should be present");

        assert!(body.contains("update_sparse_vec_model(model, fallback_entries)"));
        assert!(!body.contains("update_sliding_vec_model"));
    }

    #[test]
    fn pane_item_view_fallback_media_model_reuses_sparse_rows_for_mixed_kinds() {
        let mut initial_entries = entries_with_tile_metrics(4);
        initial_entries[0].is_dir = true;
        initial_entries[2].is_dir = true;
        let mut view = PaneView::default();

        update_pane_item_view_entries_model(
            &mut view,
            0,
            initial_entries,
            bounds_entries(0, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );
        let original_folder = view.virtual_folder_media_entries.clone();
        let original_file = view.virtual_file_media_entries.clone();

        let mut next_entries = entries_with_tile_metrics(4);
        next_entries[0].is_dir = false;
        next_entries[1].is_dir = true;
        next_entries[2].is_dir = false;
        next_entries[3].is_dir = true;
        update_pane_item_view_entries_model(
            &mut view,
            1,
            next_entries,
            bounds_entries(1, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_folder_media_entries, original_folder);
        assert_eq!(view.virtual_file_media_entries, original_file);
        assert_eq!(
            fallback_rows(&view.virtual_folder_media_entries),
            vec![(20.0, 2.0), (40.0, 6.0)]
        );
        assert_eq!(
            fallback_rows(&view.virtual_file_media_entries),
            vec![(10.0, 0.0), (30.0, 4.0)]
        );
    }

    #[test]
    fn pane_item_view_highlight_model_reuses_vec_model_when_selection_shape_changes() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(4),
            bounds_entries(0, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert!(update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));
        let original_highlights = view.virtual_highlight_entries.clone();

        assert!(update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-2".to_string()]
        ));

        assert_eq!(view.virtual_highlight_entries, original_highlights);
        assert_eq!(
            highlight_rows(&view.virtual_highlight_entries),
            vec![(20.0, 4.0, 92.0)]
        );
    }

    #[test]
    fn pane_item_view_cached_relayout_reuses_vec_model_and_selection() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            10,
            entries_with_tile_metrics(4),
            bounds_entries(10, 4),
            Vec::new(),
            Vec::new(),
            &["/tmp/item-2".to_string()],
        );
        let original = view.virtual_entries.clone();
        let original_bounds = view.virtual_bounds_entries.clone();
        let original_paint = view.virtual_paint_entries.clone();

        assert!(relayout_pane_item_view_entries_model(
            &mut view,
            11..13,
            bounds_entries(11, 2)
        ));

        assert_eq!(view.virtual_entries, original);
        assert_eq!(view.virtual_bounds_entries, original_bounds);
        assert_eq!(view.virtual_paint_entries, original_paint);
        assert_eq!(view.virtual_start_index, 11);
        assert_eq!(
            rows(&view.virtual_entries),
            vec!["/tmp/item-1".to_string(), "/tmp/item-2".to_string()]
        );
        assert_eq!(
            bounds_row_x(&view.virtual_bounds_entries),
            vec![110.0, 120.0]
        );
        assert_eq!(
            paint_rows(&view.virtual_paint_entries),
            vec![
                ("item-1".to_string(), 110.0, 56.0),
                ("item-2".to_string(), 120.0, 57.0),
            ]
        );
        assert_eq!(
            selected_token_rows(&view.virtual_entry_tokens),
            vec!["/tmp/item-2".to_string()]
        );
        assert_eq!(
            highlight_rows(&view.virtual_highlight_entries),
            vec![(120.0, 2.0, 102.0)]
        );
    }

    #[test]
    fn pane_item_view_cached_relayout_trims_sparse_media_sidecar() {
        let mut entries = entries_with_tile_metrics(4);
        entries[1].thumbnail_state = 2;
        entries[1].media_token = 101;
        entries[3].thumbnail_state = 2;
        entries[3].media_token = 103;
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            10,
            entries,
            Vec::new(),
            vec![
                media_source(1, Rgba8Pixel::new(255, 0, 0, 255)),
                media_source(3, Rgba8Pixel::new(0, 0, 255, 255)),
            ],
            Vec::new(),
            &[],
        );
        let original_media = view.virtual_media_entries.clone();

        assert_eq!(
            media_rows(&view.virtual_media_entries),
            vec![
                Rgba8Pixel::new(255, 0, 0, 255),
                Rgba8Pixel::new(0, 0, 255, 255),
            ]
        );

        assert!(relayout_pane_item_view_entries_model(
            &mut view,
            11..13,
            Vec::new()
        ));

        assert_eq!(view.virtual_media_entries, original_media);
        assert_eq!(
            media_rows(&view.virtual_media_entries),
            vec![Rgba8Pixel::new(255, 0, 0, 255)]
        );
        assert_eq!(media_tokens(&view.virtual_media_tokens), vec![(0, 101)]);
    }

    #[test]
    fn virtual_row_reuse_ignores_selection_sidecar_changes() {
        let initial_entries = (0..3).map(entry).collect::<Vec<_>>();
        let mut tokens = item_view_row_tokens(&initial_entries, &["/tmp/item-1".to_string()]);
        let model = new_item_view_entries_model(initial_entries);
        let original = model.clone();

        assert!(
            update_item_view_entries_model(
                &model,
                0,
                0,
                &mut tokens,
                (0..3).map(entry).collect(),
                &["/tmp/item-2".to_string()]
            )
            .is_none()
        );

        assert_eq!(model, original);
        assert_eq!(
            selected_token_rows(&tokens),
            vec!["/tmp/item-2".to_string()]
        );
    }

    #[test]
    fn virtual_row_reuse_compares_tokens_without_cloning_existing_rows() {
        let source = include_str!("model_update.rs");
        let body = source
            .split_once("fn update_vec_model(")
            .and_then(|(_, rest)| rest.split_once("#[cfg(test)]"))
            .map(|(body, _)| body)
            .expect("update_vec_model body should be present");
        let overlap_body = body
            .split_once("for row in overlap_rows {")
            .and_then(|(_, rest)| rest.split_once("while model.row_count() > entries.len()"))
            .map(|(body, _)| body)
            .expect("overlap loop should be present");

        assert!(
            overlap_body.contains("!current.row_equals_ignoring_selection(next)")
                && overlap_body.contains("model.set_row_data(row, entries[row].clone());")
                && !overlap_body.contains(".row_data("),
            "overlap row reuse should compare the Rust sidecar token instead of cloning current ItemViewEntry rows from Slint"
        );
    }

    #[test]
    fn metadata_model_uses_preprojected_sparse_rows() {
        let hidden = new_item_view_metadata_model(Vec::new());
        assert_eq!(hidden.row_count(), 0);

        let model = new_item_view_metadata_model(vec![
            ItemViewMetadataEntry {
                text: "Documents".into(),
                item_x: 0.0,
                item_y: 0.0,
                text_x: 52.0,
                text_width: 75.0,
                y: 2.0,
                line_height: 14.0,
                font_size: 11.0,
                is_group: true,
            },
            ItemViewMetadataEntry {
                text: "/home/user/Documents".into(),
                item_x: 0.0,
                item_y: 0.0,
                text_x: 52.0,
                text_width: 75.0,
                y: 41.0,
                line_height: 14.0,
                font_size: 11.0,
                is_group: false,
            },
        ]);

        assert_eq!(model.row_count(), 2);
        let rows = (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .map(|entry| {
                (
                    entry.text.to_string(),
                    entry.y,
                    entry.line_height,
                    entry.font_size,
                    entry.is_group,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            vec![
                ("Documents".to_string(), 2.0, 14.0, 11.0, true),
                ("/home/user/Documents".to_string(), 41.0, 14.0, 11.0, false,),
            ]
        );
    }
}
