use crate::app::geometry::ItemViewItemBounds;
use crate::app::item_view_renderer::{
    ItemViewFrameEntry, ItemViewMediaSource, ItemViewMetadataOverlaySource, ItemViewSlotProjection,
    ItemViewTileFrameBatch,
};
use crate::app::pane::PaneView;
use crate::{ItemViewEntry, ItemViewSlotEntry};
use slint::{Image, Model, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ItemViewSlotKey {
    path: String,
    occurrence: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedItemViewSlotProjection {
    key: Option<ItemViewSlotKey>,
    absolute_index: i32,
    path: SharedString,
    name: SharedString,
    media_kind: i32,
    has_metadata_group: bool,
    metadata_group: SharedString,
    has_metadata_location: bool,
    metadata_location: SharedString,
    metadata_text_x: f32,
    metadata_text_width: f32,
    metadata_group_y: f32,
    metadata_location_y: f32,
    metadata_line_height: f32,
    metadata_font_size: f32,
    x: f32,
    y: f32,
    text_width: f32,
}

impl PreparedItemViewSlotProjection {
    fn from_projection_and_key(
        projection: ItemViewSlotProjection,
        key: Option<ItemViewSlotKey>,
    ) -> Self {
        let entry = projection.entry;
        Self {
            key,
            absolute_index: projection.absolute_index,
            path: projection.path,
            name: entry.name,
            media_kind: entry.media_kind,
            has_metadata_group: entry.has_metadata_group,
            metadata_group: entry.metadata_group,
            has_metadata_location: entry.has_metadata_location,
            metadata_location: entry.metadata_location,
            metadata_text_x: entry.metadata_text_x,
            metadata_text_width: entry.metadata_text_width,
            metadata_group_y: entry.metadata_group_y,
            metadata_location_y: entry.metadata_location_y,
            metadata_line_height: entry.metadata_line_height,
            metadata_font_size: entry.metadata_font_size,
            x: entry.x,
            y: entry.y,
            text_width: entry.text_width,
        }
    }

    fn into_slot_projection_and_key(self) -> (ItemViewSlotProjection, Option<ItemViewSlotKey>) {
        let key = self.key;
        (
            ItemViewSlotProjection {
                absolute_index: self.absolute_index,
                path: self.path,
                thumbnail_token: 0,
                entry: ItemViewSlotEntry {
                    active: true,
                    name: self.name,
                    media_kind: self.media_kind,
                    has_thumbnail: false,
                    thumbnail: Image::default(),
                    has_metadata_group: self.has_metadata_group,
                    metadata_group: self.metadata_group,
                    has_metadata_location: self.has_metadata_location,
                    metadata_location: self.metadata_location,
                    metadata_text_x: self.metadata_text_x,
                    metadata_text_width: self.metadata_text_width,
                    metadata_group_y: self.metadata_group_y,
                    metadata_location_y: self.metadata_location_y,
                    metadata_line_height: self.metadata_line_height,
                    metadata_font_size: self.metadata_font_size,
                    x: self.x,
                    y: self.y,
                    text_width: self.text_width,
                },
            },
            key,
        )
    }
}

impl From<ItemViewSlotProjection> for PreparedItemViewSlotProjection {
    fn from(projection: ItemViewSlotProjection) -> Self {
        Self::from_projection_and_key(projection, None)
    }
}

impl From<PreparedItemViewSlotProjection> for ItemViewSlotProjection {
    fn from(projection: PreparedItemViewSlotProjection) -> Self {
        projection.into_slot_projection_and_key().0
    }
}

fn item_view_slot_keys(projections: &[ItemViewSlotProjection]) -> Vec<Option<ItemViewSlotKey>> {
    let mut occurrences = HashMap::new();
    projections
        .iter()
        .map(|projection| {
            if !projection.entry.active {
                return None;
            }
            let path = projection.path.to_string();
            let occurrence = occurrences.entry(path.clone()).or_insert(0);
            let key = ItemViewSlotKey {
                path,
                occurrence: *occurrence,
            };
            *occurrence += 1;
            Some(key)
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewSlotToken {
    key: Option<ItemViewSlotKey>,
    absolute_index: i32,
    thumbnail_token: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ItemViewSlotUpdateStats {
    pub(crate) active_rows: usize,
    pub(crate) inactive_rows: usize,
    pub(crate) reused_slots: usize,
    pub(crate) extended_slots: usize,
    pub(crate) patched_rows: usize,
    pub(crate) content_patched_rows: usize,
    pub(crate) geometry_patched_rows: usize,
    pub(crate) thumbnail_patched_rows: usize,
    pub(crate) thumbnail_image_reused: usize,
    pub(crate) thumbnail_image_replaced: usize,
    pub(crate) set_row_data: usize,
    pub(crate) model_extend_rows: usize,
    pub(crate) model_rebuilt_rows: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ItemViewModelUpdateStats {
    pub(crate) slot: ItemViewSlotUpdateStats,
    pub(crate) entry_rows: usize,
    pub(crate) bounds_rows: usize,
    pub(crate) media_rows: usize,
    pub(crate) metadata_rows: usize,
    pub(crate) bounds_changed: bool,
    pub(crate) raster_tokens_changed: bool,
    pub(crate) raster_revision_bumped: bool,
}

impl Default for ItemViewSlotToken {
    fn default() -> Self {
        Self {
            key: None,
            absolute_index: -1,
            thumbnail_token: 0,
        }
    }
}

impl ItemViewSlotToken {
    pub(crate) fn absolute_index(&self) -> Option<i32> {
        self.key.as_ref().map(|_| self.absolute_index)
    }

    pub(crate) fn thumbnail_token(&self) -> i32 {
        self.thumbnail_token
    }
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

    pub(crate) fn has_renderable_title(&self) -> bool {
        !self.name.as_str().trim().is_empty()
    }
}

impl ItemViewFrameEntry for ItemViewRowToken {
    fn frame_name(&self) -> SharedString {
        self.name_shared()
    }

    fn frame_path(&self) -> &str {
        self.path()
    }

    fn frame_is_dir(&self) -> bool {
        self.is_dir()
    }

    fn frame_thumbnail_state(&self) -> i32 {
        self.thumbnail_state()
    }

    fn frame_media_token(&self) -> i32 {
        self.media_token()
    }

    fn frame_selected(&self) -> bool {
        self.selected()
    }
}

pub(crate) fn new_item_view_slot_model(
    slot_entries: Vec<ItemViewSlotEntry>,
) -> ModelRc<ItemViewSlotEntry> {
    if slot_entries.is_empty() {
        return ModelRc::default();
    }

    ModelRc::new(Rc::new(VecModel::from(slot_entries)))
}

fn inactive_item_view_slot_entry() -> ItemViewSlotEntry {
    ItemViewSlotEntry {
        active: false,
        name: SharedString::new(),
        media_kind: 0,
        has_thumbnail: false,
        thumbnail: Image::default(),
        has_metadata_group: false,
        metadata_group: SharedString::new(),
        has_metadata_location: false,
        metadata_location: SharedString::new(),
        metadata_text_x: 0.0,
        metadata_text_width: 0.0,
        metadata_group_y: 0.0,
        metadata_location_y: 0.0,
        metadata_line_height: 0.0,
        metadata_font_size: 0.0,
        x: 0.0,
        y: 0.0,
        text_width: 0.0,
    }
}

struct ItemViewSlotThumbnail {
    media: Image,
    token: i32,
}

#[derive(Clone, Debug, Default)]
struct ItemViewSlotMetadata {
    group: SharedString,
    location: SharedString,
    text_x: f32,
    text_width: f32,
    group_y: f32,
    location_y: f32,
    line_height: f32,
    font_size: f32,
}

fn absolute_index_for_slice_index(start_index: usize, slice_index: i32) -> Option<i32> {
    let slice_index = usize::try_from(slice_index).ok()?;
    Some(start_index.saturating_add(slice_index) as i32)
}

fn item_view_slot_projections_with_thumbnails(
    slot_projections: Vec<PreparedItemViewSlotProjection>,
    start_index: usize,
    entries: &[ItemViewEntry],
    media_entries: Vec<ItemViewMediaSource>,
) -> Vec<(ItemViewSlotProjection, Option<ItemViewSlotKey>)> {
    let mut slot_projections = slot_projections
        .into_iter()
        .map(PreparedItemViewSlotProjection::into_slot_projection_and_key)
        .collect::<Vec<_>>();
    let mut thumbnails = HashMap::with_capacity(media_entries.len());
    for media in media_entries {
        let Some(slice_index) = usize::try_from(media.slice_index).ok() else {
            continue;
        };
        let Some(absolute_index) = absolute_index_for_slice_index(start_index, media.slice_index)
        else {
            continue;
        };
        let thumbnail_token = entries
            .get(slice_index)
            .map_or(0, |entry| entry.media_token);
        thumbnails.insert(
            absolute_index,
            ItemViewSlotThumbnail {
                token: thumbnail_token,
                media: media.media,
            },
        );
    }

    for (projection, _) in &mut slot_projections {
        if let Some(thumbnail) = thumbnails.remove(&projection.absolute_index) {
            projection.entry.has_thumbnail = true;
            projection.thumbnail_token = thumbnail.token;
            projection.entry.thumbnail = thumbnail.media;
        }
    }

    slot_projections
}

fn item_view_slot_projections_with_metadata(
    mut slot_projections: Vec<ItemViewSlotProjection>,
    start_index: usize,
    metadata_entries: Vec<ItemViewMetadataOverlaySource>,
) -> Vec<ItemViewSlotProjection> {
    let mut metadata_by_index = HashMap::<i32, ItemViewSlotMetadata>::new();
    for metadata in metadata_entries {
        let Some(absolute_index) =
            absolute_index_for_slice_index(start_index, metadata.slice_index)
        else {
            continue;
        };
        let slot_metadata = metadata_by_index.entry(absolute_index).or_default();
        slot_metadata.text_x = metadata.text_x;
        slot_metadata.text_width = metadata.text_width;
        slot_metadata.line_height = metadata.line_height;
        slot_metadata.font_size = metadata.font_size;
        if metadata.is_group {
            slot_metadata.group = metadata.text;
            slot_metadata.group_y = metadata.y;
        } else {
            slot_metadata.location = metadata.text;
            slot_metadata.location_y = metadata.y;
        }
    }

    for projection in &mut slot_projections {
        if let Some(metadata) = metadata_by_index.remove(&projection.absolute_index) {
            projection.entry.has_metadata_group = !metadata.group.is_empty();
            projection.entry.metadata_group = metadata.group;
            projection.entry.has_metadata_location = !metadata.location.is_empty();
            projection.entry.metadata_location = metadata.location;
            projection.entry.metadata_text_x = metadata.text_x;
            projection.entry.metadata_text_width = metadata.text_width;
            projection.entry.metadata_group_y = metadata.group_y;
            projection.entry.metadata_location_y = metadata.location_y;
            projection.entry.metadata_line_height = metadata.line_height;
            projection.entry.metadata_font_size = metadata.font_size;
        }
    }

    slot_projections
}

pub(crate) fn item_view_slot_projections_for_entries(
    start_index: usize,
    entries: &[ItemViewEntry],
    bounds_entries: &[ItemViewItemBounds],
    metadata_entries: Vec<ItemViewMetadataOverlaySource>,
) -> Vec<PreparedItemViewSlotProjection> {
    let frame_batch = ItemViewTileFrameBatch::from_entries_and_bounds(entries, bounds_entries, &[]);
    let slot_projections = item_view_slot_projections_with_metadata(
        frame_batch.slot_projections(start_index),
        start_index,
        metadata_entries,
    );
    let slot_keys = item_view_slot_keys(&slot_projections);
    slot_projections
        .into_iter()
        .zip(slot_keys)
        .map(|(projection, key)| {
            PreparedItemViewSlotProjection::from_projection_and_key(projection, key)
        })
        .collect()
}

fn item_view_slot_entry_matches_without_thumbnail_image(
    current: &ItemViewSlotEntry,
    next: &ItemViewSlotEntry,
) -> bool {
    item_view_slot_entry_content_matches_without_thumbnail_image(current, next)
        && item_view_slot_entry_geometry_matches(current, next)
}

fn item_view_slot_entry_content_matches_without_thumbnail_image(
    current: &ItemViewSlotEntry,
    next: &ItemViewSlotEntry,
) -> bool {
    current.active == next.active
        && current.name == next.name
        && current.media_kind == next.media_kind
        && current.has_thumbnail == next.has_thumbnail
        && current.has_metadata_group == next.has_metadata_group
        && current.metadata_group == next.metadata_group
        && current.has_metadata_location == next.has_metadata_location
        && current.metadata_location == next.metadata_location
}

fn item_view_slot_entry_geometry_matches(
    current: &ItemViewSlotEntry,
    next: &ItemViewSlotEntry,
) -> bool {
    current.metadata_location == next.metadata_location
        && current.metadata_text_x == next.metadata_text_x
        && current.metadata_text_width == next.metadata_text_width
        && current.metadata_group_y == next.metadata_group_y
        && current.metadata_location_y == next.metadata_location_y
        && current.metadata_line_height == next.metadata_line_height
        && current.metadata_font_size == next.metadata_font_size
        && current.x == next.x
        && current.y == next.y
        && current.text_width == next.text_width
}

fn update_item_view_slot_entries_model(
    view: &mut PaneView,
    slot_projections: Vec<(ItemViewSlotProjection, Option<ItemViewSlotKey>)>,
) -> ItemViewSlotUpdateStats {
    let mut stats = ItemViewSlotUpdateStats::default();
    if view.virtual_slot_entries.len() != view.virtual_slot_tokens.len() {
        view.virtual_slot_tokens =
            vec![ItemViewSlotToken::default(); view.virtual_slot_entries.len()];
    }

    let current_model = view.virtual_item_slots.clone();
    let model = current_model
        .as_any()
        .downcast_ref::<VecModel<ItemViewSlotEntry>>();

    if slot_projections.is_empty() {
        view.virtual_slot_keys.clear();
        for row in 0..view.virtual_slot_entries.len() {
            if !view.virtual_slot_entries[row].active {
                view.virtual_slot_tokens[row] = ItemViewSlotToken::default();
                continue;
            }
            let inactive = inactive_item_view_slot_entry();
            view.virtual_slot_entries[row] = inactive.clone();
            view.virtual_slot_tokens[row] = ItemViewSlotToken::default();
            if let Some(model) = model {
                model.set_row_data(row, inactive);
                stats.set_row_data += 1;
            }
            stats.patched_rows += 1;
            stats.content_patched_rows += 1;
        }
        if model.is_none() && !view.virtual_slot_entries.is_empty() {
            view.virtual_item_slots = new_item_view_slot_model(view.virtual_slot_entries.clone());
            stats.model_rebuilt_rows = view.virtual_slot_entries.len();
        }
        stats.active_rows = view
            .virtual_slot_entries
            .iter()
            .filter(|entry| entry.active)
            .count();
        stats.inactive_rows = view
            .virtual_slot_entries
            .len()
            .saturating_sub(stats.active_rows);
        return stats;
    }

    let mut old_slot_by_key = HashMap::with_capacity(view.virtual_slot_keys.len());
    for (key, &slot) in view.virtual_slot_keys.iter() {
        if slot < view.virtual_slot_entries.len() {
            old_slot_by_key.insert(key.clone(), slot);
        }
    }

    let mut assigned_slots = vec![None; slot_projections.len()];
    let mut used_slots = vec![false; view.virtual_slot_entries.len()];
    for (row, (_, key)) in slot_projections.iter().enumerate() {
        let Some(key) = key.as_ref() else {
            continue;
        };
        if let Some(&slot) = old_slot_by_key.get(key)
            && slot < used_slots.len()
            && !used_slots[slot]
        {
            assigned_slots[row] = Some(slot);
            used_slots[slot] = true;
            stats.reused_slots += 1;
        }
    }

    let mut free_slots = used_slots
        .iter()
        .enumerate()
        .filter_map(|(slot, used)| (!*used).then_some(slot))
        .collect::<Vec<_>>();
    for assigned in assigned_slots.iter_mut() {
        if assigned.is_some() {
            continue;
        }
        if let Some(slot) = free_slots.pop() {
            *assigned = Some(slot);
            used_slots[slot] = true;
        } else {
            let slot = view.virtual_slot_entries.len();
            view.virtual_slot_entries
                .push(inactive_item_view_slot_entry());
            view.virtual_slot_tokens.push(ItemViewSlotToken::default());
            used_slots.push(true);
            stats.extended_slots += 1;
            *assigned = Some(slot);
        }
    }

    let old_len = model.map_or(0, Model::row_count);
    let mut next_keys = HashMap::with_capacity(slot_projections.len());
    for ((projection, key), slot) in slot_projections.into_iter().zip(assigned_slots.into_iter()) {
        let Some(slot) = slot else {
            continue;
        };
        let Some(key) = key else {
            continue;
        };
        let mut slot_entry = projection.entry;
        let thumbnail_token = if slot_entry.has_thumbnail {
            projection.thumbnail_token
        } else {
            0
        };
        let reuses_existing_thumbnail = slot_entry.has_thumbnail
            && view
                .virtual_slot_entries
                .get(slot)
                .is_some_and(|current| current.has_thumbnail)
            && view.virtual_slot_tokens.get(slot).is_some_and(|token| {
                token.key.as_ref() == Some(&key) && token.thumbnail_token == thumbnail_token
            });
        if reuses_existing_thumbnail && let Some(current) = view.virtual_slot_entries.get(slot) {
            slot_entry.thumbnail = current.thumbnail.clone();
            stats.thumbnail_image_reused += 1;
        }
        let thumbnail_image_changed = slot_entry.has_thumbnail && !reuses_existing_thumbnail;
        let content_changed = !view.virtual_slot_entries.get(slot).is_some_and(|current| {
            item_view_slot_entry_content_matches_without_thumbnail_image(current, &slot_entry)
        });
        let geometry_changed = !content_changed
            && !view
                .virtual_slot_entries
                .get(slot)
                .is_some_and(|current| item_view_slot_entry_geometry_matches(current, &slot_entry));
        if thumbnail_image_changed
            || !view.virtual_slot_entries.get(slot).is_some_and(|current| {
                item_view_slot_entry_matches_without_thumbnail_image(current, &slot_entry)
            })
        {
            view.virtual_slot_entries[slot] = slot_entry.clone();
            if slot < old_len
                && let Some(model) = model
            {
                model.set_row_data(slot, slot_entry);
                stats.set_row_data += 1;
            }
            stats.patched_rows += 1;
            if thumbnail_image_changed {
                stats.thumbnail_patched_rows += 1;
                stats.thumbnail_image_replaced += 1;
            }
            if content_changed {
                stats.content_patched_rows += 1;
            } else if geometry_changed {
                stats.geometry_patched_rows += 1;
            }
        }
        view.virtual_slot_tokens[slot] = ItemViewSlotToken {
            key: Some(key.clone()),
            absolute_index: projection.absolute_index,
            thumbnail_token,
        };
        next_keys.insert(key, slot);
    }

    for (slot, used) in used_slots.iter().enumerate() {
        if *used {
            continue;
        }
        if view
            .virtual_slot_entries
            .get(slot)
            .is_some_and(|entry| entry.active)
        {
            let inactive = inactive_item_view_slot_entry();
            view.virtual_slot_entries[slot] = inactive.clone();
            if slot < old_len
                && let Some(model) = model
            {
                model.set_row_data(slot, inactive);
                stats.set_row_data += 1;
            }
            stats.patched_rows += 1;
            stats.content_patched_rows += 1;
        }
        if let Some(token) = view.virtual_slot_tokens.get_mut(slot) {
            *token = ItemViewSlotToken::default();
        }
    }

    view.virtual_slot_keys = next_keys;
    if let Some(model) = model {
        if old_len < view.virtual_slot_entries.len() {
            model.extend(view.virtual_slot_entries[old_len..].iter().cloned());
            stats.model_extend_rows = view.virtual_slot_entries.len() - old_len;
        }
    } else {
        view.virtual_item_slots = new_item_view_slot_model(view.virtual_slot_entries.clone());
        stats.model_rebuilt_rows = view.virtual_slot_entries.len();
    }

    stats.active_rows = view
        .virtual_slot_entries
        .iter()
        .filter(|entry| entry.active)
        .count();
    stats.inactive_rows = view
        .virtual_slot_entries
        .len()
        .saturating_sub(stats.active_rows);
    stats
}

fn item_view_row_tokens(
    entries: &[ItemViewEntry],
    selected_paths: &[String],
) -> Vec<ItemViewRowToken> {
    if selected_paths.is_empty() {
        return entries.iter().map(ItemViewRowToken::from_entry).collect();
    }

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

#[cfg(test)]
pub(crate) fn update_pane_item_view_entries_model(
    view: &mut PaneView,
    start_index: usize,
    entries: Vec<ItemViewEntry>,
    bounds_entries: Vec<ItemViewItemBounds>,
    media_entries: Vec<ItemViewMediaSource>,
    metadata_entries: Vec<ItemViewMetadataOverlaySource>,
    selected_paths: &[String],
) -> ItemViewModelUpdateStats {
    let metadata_rows = metadata_entries.len();
    let slot_projections = item_view_slot_projections_for_entries(
        start_index,
        &entries,
        &bounds_entries,
        metadata_entries,
    );
    update_pane_item_view_entries_model_with_slot_projections(
        view,
        start_index,
        entries,
        bounds_entries,
        slot_projections,
        media_entries,
        metadata_rows,
        selected_paths,
    )
}

pub(crate) fn update_pane_item_view_entries_model_with_slot_projections(
    view: &mut PaneView,
    start_index: usize,
    entries: Vec<ItemViewEntry>,
    bounds_entries: Vec<ItemViewItemBounds>,
    slot_projections: Vec<PreparedItemViewSlotProjection>,
    media_entries: Vec<ItemViewMediaSource>,
    metadata_rows: usize,
    selected_paths: &[String],
) -> ItemViewModelUpdateStats {
    let entry_rows = entries.len();
    let bounds_rows = bounds_entries.len();
    let media_rows = media_entries.len();
    let next_entry_tokens = item_view_row_tokens(&entries, selected_paths);
    let raster_tokens_changed =
        item_view_raster_tokens_changed(&view.virtual_entry_tokens, &next_entry_tokens);
    let slot_projections = item_view_slot_projections_with_thumbnails(
        slot_projections,
        start_index,
        &entries,
        media_entries,
    );
    let bounds_changed = view.virtual_bounds_entries != bounds_entries;
    let slot = update_item_view_slot_entries_model(view, slot_projections);
    view.virtual_entry_tokens = next_entry_tokens;
    view.virtual_entries = entries;
    view.virtual_bounds_entries = bounds_entries;
    view.virtual_start_index = start_index;
    let raster_revision_bumped = bounds_changed || raster_tokens_changed;
    if bounds_changed || raster_tokens_changed {
        view.bump_raster_revision();
    }
    ItemViewModelUpdateStats {
        slot,
        entry_rows,
        bounds_rows,
        media_rows,
        metadata_rows,
        bounds_changed,
        raster_tokens_changed,
        raster_revision_bumped,
    }
}

fn item_view_raster_tokens_changed(
    current: &[ItemViewRowToken],
    next: &[ItemViewRowToken],
) -> bool {
    current.len() != next.len()
        || current.iter().zip(next.iter()).any(|(current, next)| {
            match (
                current.path() == next.path(),
                current.selected() == next.selected(),
            ) {
                (true, true) => false,
                _ => true,
            }
        })
}

pub(crate) fn update_pane_item_view_selection_model(
    view: &mut PaneView,
    selected_paths: &[String],
) -> bool {
    let changed = update_item_view_selection_tokens(&mut view.virtual_entry_tokens, selected_paths);
    if changed {
        view.bump_raster_revision();
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

    fn rows(entries: &[ItemViewEntry]) -> Vec<String> {
        entries.iter().map(|entry| entry.path.to_string()).collect()
    }

    fn bounds_row_x(entries: &[ItemViewItemBounds]) -> Vec<f32> {
        entries.iter().map(|entry| entry.x).collect()
    }

    fn token_rows(tokens: &[ItemViewRowToken]) -> Vec<String> {
        tokens
            .iter()
            .map(|token| token.path().to_string())
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

    fn active_slot_rows(view: &PaneView) -> Vec<(String, f32, f32)> {
        let mut rows = view
            .virtual_slot_entries
            .iter()
            .filter(|entry| entry.active)
            .map(|entry| (entry.name.to_string(), entry.x, entry.text_width))
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.1.total_cmp(&right.1));
        rows
    }

    fn active_published_slot_rows(model: &ModelRc<ItemViewSlotEntry>) -> Vec<(String, f32, f32)> {
        let mut rows = (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .filter(|entry| entry.active)
            .map(|entry| (entry.name.to_string(), entry.x, entry.text_width))
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.1.total_cmp(&right.1));
        rows
    }

    fn slot_index_for_path(view: &PaneView, path: &str) -> Option<usize> {
        view.virtual_slot_entries
            .iter()
            .zip(view.virtual_slot_tokens.iter())
            .position(|(entry, token)| {
                entry.active
                    && token
                        .key
                        .as_ref()
                        .is_some_and(|key| key.path.as_str() == path)
            })
    }

    fn slot_index_for_name(view: &PaneView, name: &str) -> Option<usize> {
        view.virtual_slot_entries
            .iter()
            .position(|entry| entry.active && entry.name == name)
    }

    fn slot_slice_index(view: &PaneView, slot: usize) -> i32 {
        view.virtual_slot_tokens
            .get(slot)
            .and_then(ItemViewSlotToken::absolute_index)
            .unwrap_or(-1)
            .saturating_sub(view.virtual_start_index as i32)
    }

    fn thumbnail_slot_rows(view: &PaneView) -> Vec<Rgba8Pixel> {
        let mut rows = view
            .virtual_slot_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.active && entry.has_thumbnail)
            .map(|(slot, entry)| (slot_slice_index(view, slot), first_pixel(&entry.thumbnail)))
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| row.0);
        rows.into_iter().map(|(_, pixel)| pixel).collect()
    }

    fn thumbnail_slot_geometry_rows(view: &PaneView) -> Vec<(f32, f32)> {
        let mut rows = view
            .virtual_slot_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.active && entry.has_thumbnail)
            .map(|(slot, entry)| (slot_slice_index(view, slot), entry.x, entry.y))
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| row.0);
        rows.into_iter().map(|(_, x, y)| (x, y)).collect()
    }

    fn thumbnail_slot_tokens(view: &PaneView) -> Vec<(i32, i32)> {
        let mut rows = view
            .virtual_slot_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.active && entry.has_thumbnail)
            .map(|(slot, _)| {
                (
                    slot_slice_index(view, slot),
                    view.virtual_slot_tokens
                        .get(slot)
                        .map_or(0, ItemViewSlotToken::thumbnail_token),
                )
            })
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| row.0);
        rows
    }

    fn metadata_slot_rows(view: &PaneView) -> Vec<(String, bool)> {
        let mut rows = Vec::new();
        for (slot, entry) in view
            .virtual_slot_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.active)
        {
            let slice_index = slot_slice_index(view, slot);
            if entry.has_metadata_group {
                rows.push((slice_index, 0, entry.metadata_group.to_string(), true));
            }
            if entry.has_metadata_location {
                rows.push((slice_index, 1, entry.metadata_location.to_string(), false));
            }
        }
        rows.sort_by_key(|row| (row.0, row.1));
        rows.into_iter()
            .map(|(_, _, text, is_group)| (text, is_group))
            .collect()
    }

    fn metadata_slot_geometry_rows(view: &PaneView) -> Vec<(f32, f32)> {
        let mut rows = Vec::new();
        for (slot, entry) in view
            .virtual_slot_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.active)
        {
            let slice_index = slot_slice_index(view, slot);
            if entry.has_metadata_group {
                rows.push((slice_index, 0, entry.x, entry.y));
            }
            if entry.has_metadata_location {
                rows.push((slice_index, 1, entry.x, entry.y));
            }
        }
        rows.sort_by_key(|row| (row.0, row.1));
        rows.into_iter().map(|(_, _, x, y)| (x, y)).collect()
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
    fn slot_overlays_carry_projected_item_bounds() {
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

        assert_eq!(thumbnail_slot_geometry_rows(&view), vec![(210.0, 2.0)]);
        assert_eq!(metadata_slot_geometry_rows(&view), vec![(210.0, 2.0)]);
        assert_eq!(metadata_slot_rows(&view), vec![("Group".to_string(), true)]);
    }

    #[test]
    fn tile_frame_sources_drive_split_primitive_models() {
        let mut entries = entries_with_tile_metrics(3);
        entries[0].is_dir = true;
        entries[1].media_token = 77;
        let bounds = bounds_entries(10, 3);

        let frame_batch = ItemViewTileFrameBatch::from_entries_and_bounds(
            &entries,
            &bounds,
            &["/tmp/item-1".to_string()],
        );
        let projections = frame_batch.slot_projections(10);

        assert_eq!(frame_batch.sources().len(), 3);
        assert_eq!(projections.len(), 3);
        assert_eq!(projections[1].absolute_index, 11);
        assert_eq!(projections[1].path, "/tmp/item-1");
        assert_eq!(projections[1].entry.name, "item-1");
        assert_eq!(projections[1].entry.x, 110.0);
        assert_eq!(projections[1].entry.text_width, 56.0);
        assert_eq!(frame_batch.media_token_for_slice_index(1), 77);
    }

    #[test]
    fn item_view_row_token_can_feed_tile_frame_batch_trait() {
        let entries = entries_with_tile_metrics(1);
        let tokens = item_view_row_tokens(&entries, &["/tmp/item-0".to_string()]);
        let bounds = bounds_entries(5, 1);

        let batch = ItemViewTileFrameBatch::from_bounded_entries(&tokens, &bounds);
        let frame = &batch.sources()[0];

        assert_eq!(frame.name, "item-0");
        assert!(frame.selected);
        assert_eq!(frame.x, 50.0);
        assert_eq!(frame.text_width, 50.0);
    }

    #[test]
    fn renderer_batch_owns_slot_entry_projection() {
        let source = include_str!("model_update.rs");
        let obsolete_helper = ["fn item_view_", "paint_entries("].concat();

        assert!(!source.contains(&obsolete_helper));
        assert!(source.contains("frame_batch.slot_projections(start_index)"));
    }

    #[test]
    fn renderer_owns_metadata_text_projection() {
        let source = include_str!("model_update.rs");
        let obsolete_helper = ["fn project_metadata_", "entries_with_bounds("].concat();
        let obsolete_renderer_projection = ["metadata_entries_", "with_bounds"].concat();

        assert!(!source.contains(&obsolete_helper));
        assert!(!source.contains(&obsolete_renderer_projection));
        assert!(
            source.contains("item_view_slot_projections_with_metadata(")
                && source.contains("PreparedItemViewSlotProjection"),
            "metadata projection should attach renderer-owned source rows to stable item slots before UI-thread model patching"
        );
    }

    #[test]
    fn renderer_owns_raster_media_and_metadata_overlay_types() {
        let source = include_str!("model_update.rs");
        for obsolete_type in [
            ["struct ItemView", "MediaSource"].concat(),
            ["struct ItemView", "MediaToken"].concat(),
            ["struct ItemView", "RasterMediaEntry"].concat(),
            ["struct ItemView", "MetadataOverlaySource"].concat(),
        ] {
            assert!(!source.contains(&obsolete_type));
        }
    }

    #[test]
    fn pane_item_view_entry_sidecar_updates_each_view_independently() {
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
    fn pane_item_view_entry_sidecar_replaces_slice_when_range_slides() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            (0..6).map(entry).collect(),
            bounds_entries(0, 6),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_start_index, 0);
        assert_eq!(
            rows(&view.virtual_entries),
            (0..6)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            token_rows(&view.virtual_entry_tokens),
            rows(&view.virtual_entries)
        );
        assert_eq!(
            bounds_row_x(&view.virtual_bounds_entries),
            vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0]
        );

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..8).map(entry).collect(),
            bounds_entries(2, 6),
            Vec::new(),
            Vec::new(),
            &["/tmp/item-4".to_string()],
        );

        assert_eq!(view.virtual_start_index, 2);
        assert_eq!(
            rows(&view.virtual_entries),
            (2..8)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            token_rows(&view.virtual_entry_tokens),
            rows(&view.virtual_entries)
        );
        assert_eq!(
            selected_token_rows(&view.virtual_entry_tokens),
            vec!["/tmp/item-4".to_string()]
        );
        assert_eq!(
            bounds_row_x(&view.virtual_bounds_entries),
            vec![20.0, 30.0, 40.0, 50.0, 60.0, 70.0]
        );

        update_pane_item_view_entries_model(
            &mut view,
            20,
            (20..23).map(entry).collect(),
            bounds_entries(20, 3),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_start_index, 20);
        assert_eq!(
            rows(&view.virtual_entries),
            (20..23)
                .map(|index| format!("/tmp/item-{index}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(view.virtual_entry_tokens.len(), 3);
    }

    #[test]
    fn pane_item_view_entry_sidecar_updates_media_token_without_row_image_model() {
        let mut view = PaneView::default();
        let mut old_entry = entry(0);
        old_entry.thumbnail_state = 2;
        old_entry.media_token = 42;
        update_pane_item_view_entries_model(
            &mut view,
            0,
            vec![old_entry],
            bounds_entries(0, 1),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_entries[0].media_token, 42);
        assert_eq!(view.virtual_entry_tokens[0].media_token(), 42);

        let mut new_entry = entry(0);
        new_entry.thumbnail_state = 2;
        new_entry.media_token = 43;
        update_pane_item_view_entries_model(
            &mut view,
            0,
            vec![new_entry],
            bounds_entries(0, 1),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_entries[0].media_token, 43);
        assert_eq!(view.virtual_entry_tokens[0].media_token(), 43);
    }

    #[test]
    fn pane_item_view_entry_sidecar_updates_selection_tokens_without_replacing_entries() {
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
        assert!(!update_pane_item_view_selection_model(
            &mut view,
            &["/tmp/item-1".to_string(), "/tmp/item-3".to_string()]
        ));

        assert!(update_pane_item_view_selection_model(&mut view, &[]));
        assert!(selected_token_rows(&view.virtual_entry_tokens).is_empty());
    }

    #[test]
    fn pane_item_view_bounds_sidecar_updates_without_slint_model() {
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
        let original_revision = view.raster_revision_for_test();

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..6).map(entry).collect(),
            bounds_entries(2, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_ne!(view.virtual_bounds_entries, original_bounds);
        assert_eq!(
            bounds_row_x(&view.virtual_bounds_entries),
            vec![20.0, 30.0, 40.0, 50.0]
        );
        let updated_revision = view.raster_revision_for_test();
        assert!(updated_revision > original_revision);

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..6).map(entry).collect(),
            bounds_entries(2, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.raster_revision_for_test(), updated_revision);

        update_pane_item_view_entries_model(
            &mut view,
            1,
            (1..5).map(entry).collect(),
            bounds_entries(1, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(
            bounds_row_x(&view.virtual_bounds_entries),
            vec![10.0, 20.0, 30.0, 40.0]
        );
    }

    #[test]
    fn pane_item_view_slot_model_reuses_vec_model_when_range_slides() {
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
        let original_slots = view.virtual_item_slots.clone();

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..6).map(entry).collect(),
            bounds_entries(2, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_item_slots, original_slots);
        assert_eq!(
            active_slot_rows(&view),
            vec![
                ("item-2".to_string(), 20.0, 47.0),
                ("item-3".to_string(), 30.0, 48.0),
                ("item-4".to_string(), 40.0, 49.0),
                ("item-5".to_string(), 50.0, 50.0),
            ]
        );
        assert_eq!(
            active_published_slot_rows(&view.virtual_item_slots),
            active_slot_rows(&view)
        );
        let item_2_slot = slot_index_for_path(&view, "/tmp/item-2")
            .expect("item-2 should remain visible after forward slide");
        let item_3_slot = slot_index_for_path(&view, "/tmp/item-3")
            .expect("item-3 should remain visible after forward slide");

        update_pane_item_view_entries_model(
            &mut view,
            1,
            (1..5).map(entry).collect(),
            bounds_entries(1, 4),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(view.virtual_item_slots, original_slots);
        assert_eq!(
            active_slot_rows(&view),
            vec![
                ("item-1".to_string(), 10.0, 46.0),
                ("item-2".to_string(), 20.0, 47.0),
                ("item-3".to_string(), 30.0, 48.0),
                ("item-4".to_string(), 40.0, 49.0),
            ]
        );
        assert_eq!(slot_index_for_path(&view, "/tmp/item-2"), Some(item_2_slot));
        assert_eq!(slot_index_for_path(&view, "/tmp/item-3"), Some(item_3_slot));
    }

    #[test]
    fn pane_item_view_slot_keys_keep_duplicate_paths_distinct() {
        let mut entries = vec![entry(0), entry(1)];
        entries[0].name = "duplicate-a".into();
        entries[0].path = "/tmp/duplicate".into();
        entries[1].name = "duplicate-b".into();
        entries[1].path = "/tmp/duplicate".into();
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries.clone(),
            bounds_entries(0, 2),
            Vec::new(),
            Vec::new(),
            &[],
        );
        let duplicate_a_slot =
            slot_index_for_name(&view, "duplicate-a").expect("first duplicate should have a slot");
        let duplicate_b_slot =
            slot_index_for_name(&view, "duplicate-b").expect("second duplicate should have a slot");

        entries[0].media_token = 11;
        entries[1].media_token = 22;
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries,
            bounds_entries(0, 2),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert_eq!(
            slot_index_for_name(&view, "duplicate-a"),
            Some(duplicate_a_slot)
        );
        assert_eq!(
            slot_index_for_name(&view, "duplicate-b"),
            Some(duplicate_b_slot)
        );
        assert_ne!(duplicate_a_slot, duplicate_b_slot);
    }

    #[test]
    fn pane_item_view_slot_metadata_reuses_slot_model_for_updates() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(3),
            bounds_entries(0, 3),
            Vec::new(),
            metadata_entries(0, 3),
            &[],
        );
        let original_slots = view.virtual_item_slots.clone();

        update_pane_item_view_entries_model(
            &mut view,
            2,
            (2..5).map(entry).collect(),
            bounds_entries(2, 3),
            Vec::new(),
            metadata_entries(2, 3),
            &[],
        );

        assert_eq!(view.virtual_item_slots, original_slots);
        assert_eq!(
            metadata_slot_rows(&view),
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
            bounds_entries(2, 3),
            Vec::new(),
            metadata_entries(2, 3),
            &[],
        );

        assert_eq!(view.virtual_item_slots, original_slots);
    }

    #[test]
    fn pane_item_view_slot_metadata_clears_rows_without_stale_text() {
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(2),
            bounds_entries(0, 2),
            Vec::new(),
            metadata_entries(0, 2),
            &[],
        );
        assert_eq!(metadata_slot_rows(&view).len(), 4);

        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries_with_tile_metrics(2),
            bounds_entries(0, 2),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert!(metadata_slot_rows(&view).is_empty());
        assert!(
            view.virtual_slot_entries
                .iter()
                .filter(|entry| entry.active)
                .all(|entry| !entry.has_metadata_group
                    && entry.metadata_group.is_empty()
                    && !entry.has_metadata_location
                    && entry.metadata_location.is_empty())
        );
    }

    #[test]
    fn pane_item_view_thumbnail_overlay_uses_tokens_without_image_comparison_or_raster_bump() {
        let mut entries = entries_with_tile_metrics(4);
        entries[1].thumbnail_state = 2;
        entries[1].media_token = 101;
        entries[3].thumbnail_state = 2;
        entries[3].media_token = 103;
        let mut view = PaneView::default();
        let initial_raster_revision = view.raster_revision_for_test();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            entries,
            bounds_entries(0, 4),
            vec![
                media_source(1, Rgba8Pixel::new(255, 0, 0, 255)),
                media_source(3, Rgba8Pixel::new(0, 0, 255, 255)),
            ],
            Vec::new(),
            &[],
        );
        let layout_raster_revision = view.raster_revision_for_test();
        assert!(layout_raster_revision > initial_raster_revision);
        assert_eq!(
            thumbnail_slot_rows(&view),
            vec![
                Rgba8Pixel::new(255, 0, 0, 255),
                Rgba8Pixel::new(0, 0, 255, 255),
            ]
        );

        let mut same_token_entries = entries_with_tile_metrics(4);
        same_token_entries[1].thumbnail_state = 2;
        same_token_entries[1].media_token = 101;
        same_token_entries[3].thumbnail_state = 2;
        same_token_entries[3].media_token = 103;
        update_pane_item_view_entries_model(
            &mut view,
            0,
            same_token_entries,
            bounds_entries(0, 4),
            vec![
                media_source(1, Rgba8Pixel::new(0, 255, 0, 255)),
                media_source(3, Rgba8Pixel::new(255, 255, 0, 255)),
            ],
            Vec::new(),
            &[],
        );
        assert_eq!(view.raster_revision_for_test(), layout_raster_revision);
        assert_eq!(
            thumbnail_slot_rows(&view),
            vec![
                Rgba8Pixel::new(255, 0, 0, 255),
                Rgba8Pixel::new(0, 0, 255, 255),
            ]
        );

        let mut updated_entries = entries_with_tile_metrics(4);
        updated_entries[1].thumbnail_state = 2;
        updated_entries[1].media_token = 201;
        updated_entries[3].thumbnail_state = 2;
        updated_entries[3].media_token = 203;
        update_pane_item_view_entries_model(
            &mut view,
            0,
            updated_entries,
            bounds_entries(0, 4),
            vec![
                media_source(1, Rgba8Pixel::new(0, 255, 0, 255)),
                media_source(3, Rgba8Pixel::new(255, 255, 0, 255)),
            ],
            Vec::new(),
            &[],
        );

        assert_eq!(view.raster_revision_for_test(), layout_raster_revision);
        assert_eq!(
            thumbnail_slot_rows(&view),
            vec![
                Rgba8Pixel::new(0, 255, 0, 255),
                Rgba8Pixel::new(255, 255, 0, 255),
            ]
        );
        assert_eq!(thumbnail_slot_tokens(&view), vec![(1, 201), (3, 203)]);
    }

    #[test]
    fn item_view_entry_and_bounds_sidecars_do_not_publish_slint_models() {
        let source = include_str!("model_update.rs");
        let production_source = source
            .split_once("#[cfg(test)]\nmod tests")
            .map(|(body, _)| body)
            .expect("model_update.rs should contain tests after production code");

        for obsolete in [
            "fn new_item_view_entries_model(",
            "fn new_item_view_bounds_model(",
            "fn update_item_view_entries_model(",
            "fn update_item_view_bounds_entries_model(",
            "fn update_vec_model(",
            "fn update_sliding_vec_model(",
        ] {
            assert!(
                !production_source.contains(obsolete),
                "{obsolete} should not exist on the item-view hot path"
            );
        }
        assert!(production_source.contains("view.virtual_entries = entries;"));
        assert!(production_source.contains("view.virtual_bounds_entries = bounds_entries;"));
        assert!(
            production_source
                .contains("update_item_view_slot_entries_model(view, slot_projections);")
        );
    }

    #[test]
    fn slot_metadata_uses_preprojected_source_rows() {
        let entries = entries_with_tile_metrics(1);
        let bounds = bounds_entries(0, 1);
        let frame_batch = ItemViewTileFrameBatch::from_entries_and_bounds(&entries, &bounds, &[]);
        let projections = item_view_slot_projections_with_metadata(
            frame_batch.slot_projections(0),
            0,
            vec![
                ItemViewMetadataOverlaySource {
                    slice_index: 0,
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
                ItemViewMetadataOverlaySource {
                    slice_index: 0,
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
            ],
        );

        assert_eq!(projections.len(), 1);
        let slot = &projections[0].entry;
        assert!(slot.has_metadata_group);
        assert_eq!(slot.metadata_group, "Documents");
        assert!(slot.has_metadata_location);
        assert_eq!(slot.metadata_location, "/home/user/Documents");
        assert_eq!(slot.metadata_text_x, 52.0);
        assert_eq!(slot.metadata_text_width, 75.0);
        assert_eq!(slot.metadata_group_y, 2.0);
        assert_eq!(slot.metadata_location_y, 41.0);
        assert_eq!(slot.metadata_line_height, 14.0);
        assert_eq!(slot.metadata_font_size, 11.0);
    }
}
