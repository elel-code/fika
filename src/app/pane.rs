#[cfg(test)]
use crate::FileEntry;
#[cfg(test)]
use crate::app::geometry::compact_text_width_units;
use crate::app::geometry::{ItemViewItemBounds, ItemViewLayoutEngine, ItemViewLayouter};
use crate::app::item_view::ItemViewInputState;
use crate::app::item_view_model::{
    ItemViewModelEntry, ItemViewModelEntryArc, item_view_model_entries_equal,
};
use crate::app::item_view_perf::{self, PerfTimer};
#[cfg(test)]
use crate::app::item_view_renderer::ItemViewMediaSource;
use crate::app::item_view_renderer::{
    ITEM_VIEW_MEDIA_KIND_ARCHIVE, ITEM_VIEW_MEDIA_KIND_AUDIO, ITEM_VIEW_MEDIA_KIND_CODE,
    ITEM_VIEW_MEDIA_KIND_EXECUTABLE, ITEM_VIEW_MEDIA_KIND_FILE, ITEM_VIEW_MEDIA_KIND_FOLDER,
    ITEM_VIEW_MEDIA_KIND_IMAGE, ITEM_VIEW_MEDIA_KIND_PDF, ITEM_VIEW_MEDIA_KIND_TEXT,
    ITEM_VIEW_MEDIA_KIND_VIDEO, ItemViewRenderMetrics, ItemViewTileFrameBatch,
    ItemViewTileFrameRaster, ItemViewTileFrameRasterInput, render_fallback_media_icon,
};
use crate::app::model_update::{ItemViewRowToken, ItemViewSlotKey, ItemViewSlotToken};
use crate::app::virtual_view::VirtualViewSnapshotInput;
#[cfg(test)]
use crate::fs::entries::RawFileEntry;
use crate::fs::{file_ops, search, thumbnails};
use crate::support::generation::GenerationCounter;
use crate::{ItemViewEntry, ItemViewSlotEntry};
use slint::{Image, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::ops::{Index, Range};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub(crate) const MAX_VIEW_STATE_CACHE_ENTRIES: usize = 128;

#[derive(Debug)]
pub(crate) struct PaneState {
    pub(crate) id: u64,
    pub(crate) current_dir: PathBuf,
    pub(crate) path_input_text: String,
    pub(crate) path_focused: bool,
    pub(crate) status: String,
    pub(crate) entries: PaneEntryModel,
    pub(crate) history: PaneHistory,
    pub(crate) selection: PaneSelection,
    pub(crate) search: PaneSearch,
    pub(crate) search_cancel: Option<Arc<AtomicBool>>,
    pub(crate) search_progress: search::SearchProgress,
    pub(crate) search_generation: GenerationCounter,
    pub(crate) search_index_generation: GenerationCounter,
    pub(crate) load_generation: GenerationCounter,
    pub(crate) open_generation: GenerationCounter,
    pub(crate) thumbnail_generation: GenerationCounter,
    pub(crate) view: PaneView,
}

impl PaneState {
    #[cfg(test)]
    pub(crate) fn new(current_dir: PathBuf) -> Self {
        Self::new_with_id(0, current_dir)
    }

    fn new_with_id(id: u64, current_dir: PathBuf) -> Self {
        Self {
            id,
            path_input_text: current_dir.display().to_string(),
            current_dir,
            path_focused: false,
            status: String::new(),
            entries: PaneEntryModel::default(),
            history: PaneHistory::default(),
            selection: PaneSelection::default(),
            search: PaneSearch::default(),
            search_cancel: None,
            search_progress: search::SearchProgress::default(),
            search_generation: GenerationCounter::default(),
            search_index_generation: GenerationCounter::default(),
            load_generation: GenerationCounter::default(),
            open_generation: GenerationCounter::default(),
            thumbnail_generation: GenerationCounter::default(),
            view: PaneView::default(),
        }
    }

    pub(crate) fn split_snapshot(&self, id: u64) -> Self {
        let mut pane = Self::new_with_id(id, self.current_dir.clone());
        pane.set_entries(self.entries.clone());
        pane.search = self.search.clone();
        if pane.search.index_pending {
            pane.search.index_pending = false;
            pane.search.visible_entry_indices = None;
            pane.search.visible_location_groups = None;
        }
        pane.view.viewport_x = self.view.viewport_x;
        pane.view.virtual_view = self.view.virtual_view.clone();
        pane.view.virtual_entries = self.view.virtual_entries.clone();
        pane.view.virtual_bounds_entries = self.view.virtual_bounds_entries.clone();
        pane.view.virtual_item_slots = clone_item_view_slot_model(&self.view.virtual_item_slots);
        pane.view.virtual_slot_entries = self.view.virtual_slot_entries.clone();
        pane.view.virtual_slot_tokens =
            clone_item_view_slot_tokens_without_selection(&self.view.virtual_slot_tokens);
        pane.view.virtual_slot_keys = self.view.virtual_slot_keys.clone();
        pane.view.virtual_entry_tokens =
            clone_item_view_row_tokens_without_selection(&self.view.virtual_entry_tokens);
        pane.view.virtual_start_index = self.view.virtual_start_index;
        pane.view.raster_updates_deferred = self.view.raster_updates_deferred;
        pane
    }

    pub(crate) fn set_entries(&mut self, entries: PaneEntryModel) {
        let has_locations = entries.iter().any(ItemViewModelEntry::model_has_location);
        self.set_entries_with_location_state(entries, has_locations);
    }

    pub(crate) fn set_entries_with_location_state(
        &mut self,
        entries: PaneEntryModel,
        has_locations: bool,
    ) {
        self.entries = entries;
        self.search.visible_entry_indices = None;
        self.search.visible_entries_have_locations = has_locations;
        self.search.visible_location_groups = None;
        self.search.index_pending = false;
        self.search_index_generation.next();
        self.view.clear_virtual_view();
    }

    pub(crate) fn clear_entries(&mut self) {
        self.entries = PaneEntryModel::default();
        self.search.visible_entry_indices = None;
        self.search.visible_entries_have_locations = false;
        self.search.visible_location_groups = None;
        self.search.index_pending = false;
        self.search_index_generation.next();
        self.view.virtual_entries.clear();
        self.view.virtual_bounds_entries.clear();
        self.view.virtual_item_slots = ModelRc::default();
        self.view.virtual_slot_entries.clear();
        self.view.virtual_slot_tokens.clear();
        self.view.virtual_slot_keys.clear();
        self.view.virtual_entry_tokens.clear();
        self.view.drop_target_slice_index = None;
        self.view.clear_raster_cache();
        self.view.virtual_start_index = 0;
        self.view.clear_virtual_view();
    }

    pub(crate) fn entry_model(&self) -> PaneEntryModel {
        self.entries.clone()
    }

    pub(crate) fn show_item_locations(&self) -> bool {
        file_ops::is_trash_files_dir(&self.current_dir)
            || (self.search.recursive && !self.search.query.is_empty())
    }

    pub(crate) fn item_view_text_line_count(&self) -> usize {
        if self.show_item_locations() { 3 } else { 1 }
    }

    #[cfg(test)]
    pub(crate) fn set_file_entries(&mut self, entries: Vec<FileEntry>) {
        self.set_entries(PaneEntryModel::from_entries(entries));
    }
}

fn clone_item_view_slot_model(model: &ModelRc<ItemViewSlotEntry>) -> ModelRc<ItemViewSlotEntry> {
    let entries = (0..model.row_count())
        .filter_map(|row| model.row_data(row))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        ModelRc::default()
    } else {
        ModelRc::new(Rc::new(VecModel::from(entries)))
    }
}

fn clone_item_view_row_tokens_without_selection(
    tokens: &[ItemViewRowToken],
) -> Vec<ItemViewRowToken> {
    tokens
        .iter()
        .cloned()
        .map(|mut token| {
            token.set_selected(false);
            token
        })
        .collect()
}

fn clone_item_view_slot_tokens_without_selection(
    tokens: &[ItemViewSlotToken],
) -> Vec<ItemViewSlotToken> {
    tokens.to_vec()
}

#[derive(Clone, Default)]
pub(crate) struct PaneEntryModel {
    entries: Arc<[ItemViewModelEntryArc]>,
}

impl PaneEntryModel {
    pub(crate) fn new(entries: Vec<ItemViewModelEntryArc>) -> Self {
        Self {
            entries: Arc::from(entries),
        }
    }

    pub(crate) fn from_entries<T>(entries: impl IntoIterator<Item = T>) -> Self
    where
        T: ItemViewModelEntry + Send + Sync + 'static,
    {
        Self::new(
            entries
                .into_iter()
                .map(|entry| Arc::new(entry) as ItemViewModelEntryArc)
                .collect(),
        )
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &dyn ItemViewModelEntry> + '_ {
        self.entries
            .iter()
            .map(|entry| entry.as_ref() as &dyn ItemViewModelEntry)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn get(&self, index: usize) -> Option<&dyn ItemViewModelEntry> {
        self.entries
            .get(index)
            .map(|entry| entry.as_ref() as &dyn ItemViewModelEntry)
    }

    pub(crate) fn entry_arc(&self, index: usize) -> Option<ItemViewModelEntryArc> {
        self.entries.get(index).cloned()
    }

    pub(crate) fn entry_arcs_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = ItemViewModelEntryArc> + '_ {
        let start = range.start.min(self.entries.len());
        let end = range.end.min(self.entries.len());
        self.entries[start..end].iter().cloned()
    }
}

impl From<Vec<ItemViewModelEntryArc>> for PaneEntryModel {
    fn from(entries: Vec<ItemViewModelEntryArc>) -> Self {
        Self::new(entries)
    }
}

impl Index<usize> for PaneEntryModel {
    type Output = dyn ItemViewModelEntry + Send + Sync;

    fn index(&self, index: usize) -> &Self::Output {
        self.entries[index].as_ref()
    }
}

impl fmt::Debug for PaneEntryModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaneEntryModel")
            .field("len", &self.entries.len())
            .finish()
    }
}

impl PartialEq for PaneEntryModel {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .iter()
                .zip(other.iter())
                .all(|(left, right)| item_view_model_entries_equal(left, right))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PreparedDirectoryEntries {
    pub(crate) entries: PaneEntryModel,
    pub(crate) has_locations: bool,
}

impl PreparedDirectoryEntries {
    pub(crate) fn new<T>(entries: Vec<T>) -> Self
    where
        T: ItemViewModelEntry + Send + Sync + 'static,
    {
        let has_locations = entries.iter().any(ItemViewModelEntry::model_has_location);
        Self {
            entries: PaneEntryModel::from_entries(entries),
            has_locations,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug)]
pub(crate) struct PanesState {
    panes: Vec<PaneState>,
    focused_slot: usize,
    next_pane_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneTarget {
    Focused,
    Slot(i32),
    Id(u64),
}

impl PanesState {
    pub(crate) fn new(active_dir: PathBuf) -> Self {
        Self {
            panes: vec![PaneState::new_with_id(1, active_dir)],
            focused_slot: 0,
            next_pane_id: 2,
        }
    }

    pub(crate) fn is_split(&self) -> bool {
        self.panes.len() > 1
    }

    #[cfg(test)]
    pub(crate) fn pane_count(&self) -> usize {
        self.panes.len()
    }

    pub(crate) fn pane_by_id(&self, id: u64) -> Option<&PaneState> {
        self.panes.iter().find(|pane| pane.id == id)
    }

    pub(crate) fn pane_mut_by_id(&mut self, id: u64) -> Option<&mut PaneState> {
        self.panes.iter_mut().find(|pane| pane.id == id)
    }

    pub(crate) fn slot_for_id(&self, id: u64) -> Option<i32> {
        self.panes
            .iter()
            .position(|pane| pane.id == id)
            .map(|slot| slot as i32)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (i32, &PaneState)> + '_ {
        self.panes
            .iter()
            .enumerate()
            .map(|(slot, pane)| (slot as i32, pane))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (i32, &mut PaneState)> + '_ {
        self.panes
            .iter_mut()
            .enumerate()
            .map(|(slot, pane)| (slot as i32, pane))
    }

    pub(crate) fn focused_slot(&self) -> i32 {
        self.focused_slot_index() as i32
    }

    pub(crate) fn focus_slot(&mut self, slot: i32) -> bool {
        let Ok(slot) = usize::try_from(slot) else {
            return false;
        };
        if slot >= self.panes.len() {
            return false;
        }
        self.focused_slot = slot;
        true
    }

    pub(crate) fn pane_for_slot(&self, slot: i32) -> Option<&PaneState> {
        usize::try_from(slot)
            .ok()
            .and_then(|slot| self.panes.get(slot))
    }

    pub(crate) fn pane_mut_for_slot(&mut self, slot: i32) -> Option<&mut PaneState> {
        usize::try_from(slot)
            .ok()
            .and_then(|slot| self.panes.get_mut(slot))
    }

    pub(crate) fn focused(&self) -> &PaneState {
        &self.panes[self.focused_slot_index()]
    }

    pub(crate) fn focused_mut(&mut self) -> &mut PaneState {
        let slot = self.focused_slot_index();
        &mut self.panes[slot]
    }

    pub(crate) fn pane_for_target(&self, target: PaneTarget) -> Option<&PaneState> {
        match target {
            PaneTarget::Focused => self.pane_for_slot(self.focused_slot()),
            PaneTarget::Slot(slot) => self.pane_for_slot(slot),
            PaneTarget::Id(id) => self.pane_by_id(id),
        }
    }

    pub(crate) fn pane_mut_for_target(&mut self, target: PaneTarget) -> Option<&mut PaneState> {
        match target {
            PaneTarget::Focused => self.pane_mut_for_slot(self.focused_slot()),
            PaneTarget::Slot(slot) => self.pane_mut_for_slot(slot),
            PaneTarget::Id(id) => self.pane_mut_by_id(id),
        }
    }

    #[cfg(test)]
    pub(crate) fn open_pane(&mut self, current_dir: PathBuf) -> bool {
        let id = self.allocate_pane_id();
        self.panes.push(PaneState::new_with_id(id, current_dir));
        true
    }

    pub(crate) fn open_peer_from_focused(&mut self) -> bool {
        let id = self.allocate_pane_id();
        let focused = self.focused_slot_index();
        self.panes
            .insert(focused + 1, self.focused().split_snapshot(id));
        self.focused_slot = focused;
        true
    }

    pub(crate) fn close_focused_pane_slot(&mut self) -> Option<(i32, PaneState)> {
        if self.panes.len() <= 1 {
            return None;
        }
        let slot = self.focused_slot_index();
        let closed = self.panes.remove(slot);
        self.focused_slot = slot.saturating_sub(1).min(self.panes.len() - 1);
        Some((slot as i32, closed))
    }

    pub(crate) fn prune_mount_path(&mut self, mount_path: &Path, fallback_dir: PathBuf) -> bool {
        let mut slot_zero_moved = false;
        for (slot, pane) in self.panes.iter_mut().enumerate() {
            let moved = prune_pane_mount_path(pane, mount_path, &fallback_dir);
            slot_zero_moved |= slot == 0 && moved;
        }
        slot_zero_moved
    }

    fn allocate_pane_id(&mut self) -> u64 {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        id
    }

    fn focused_slot_index(&self) -> usize {
        self.focused_slot.min(self.panes.len().saturating_sub(1))
    }
}

fn prune_pane_mount_path(pane: &mut PaneState, mount_path: &Path, fallback_dir: &Path) -> bool {
    pane.history.prune_under(mount_path);
    if !pane.current_dir.starts_with(mount_path) {
        return false;
    }
    pane.current_dir = fallback_dir.to_path_buf();
    true
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaneNavigation {
    pub(crate) previous: PathBuf,
    pub(crate) target: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneSelection {
    pub(crate) paths: Vec<String>,
    pub(crate) anchor: Option<String>,
}

impl PaneSelection {
    pub(crate) fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneSearch {
    pub(crate) bar_open: bool,
    pub(crate) loading: bool,
    pub(crate) index_pending: bool,
    pub(crate) focus_request: i32,
    pub(crate) query_sync_request: i32,
    pub(crate) query: String,
    pub(crate) recursive: bool,
    pub(crate) kind_filter: i32,
    pub(crate) modified_filter: i32,
    pub(crate) size_filter: i32,
    pub(crate) visible_entry_indices: Option<Arc<[usize]>>,
    pub(crate) visible_entries_have_locations: bool,
    pub(crate) visible_location_groups: Option<Arc<[String]>>,
}

impl PaneSearch {
    pub(crate) fn filters_active(&self) -> bool {
        self.kind_filter != 0 || self.modified_filter != 0 || self.size_filter != 0
    }

    pub(crate) fn panel_visible(&self) -> bool {
        self.bar_open || self.loading || !self.query.is_empty() || self.filters_active()
    }

    pub(crate) fn request_focus(&mut self) {
        self.focus_request = self.focus_request.saturating_add(1);
    }

    pub(crate) fn request_query_sync(&mut self) {
        self.query_sync_request = self.query_sync_request.saturating_add(1);
    }

    pub(crate) fn reset_all(&mut self) {
        self.bar_open = false;
        self.loading = false;
        self.index_pending = false;
        self.focus_request = 0;
        self.query.clear();
        self.recursive = false;
        self.kind_filter = 0;
        self.modified_filter = 0;
        self.size_filter = 0;
        self.visible_entry_indices = None;
        self.visible_entries_have_locations = false;
        self.visible_location_groups = None;
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaneView {
    pub(crate) viewport_x: f32,
    pub(crate) input: ItemViewInputState,
    pub(crate) virtual_view: VirtualViewCache,
    pub(crate) virtual_generation: GenerationCounter,
    pub(crate) virtual_entries: Vec<ItemViewEntry>,
    pub(crate) virtual_bounds_entries: Vec<ItemViewItemBounds>,
    pub(crate) virtual_item_slots: ModelRc<ItemViewSlotEntry>,
    pub(crate) virtual_slot_entries: Vec<ItemViewSlotEntry>,
    pub(crate) virtual_slot_tokens: Vec<ItemViewSlotToken>,
    pub(crate) virtual_slot_keys: HashMap<ItemViewSlotKey, usize>,
    pub(crate) virtual_entry_tokens: Vec<ItemViewRowToken>,
    pub(crate) virtual_start_index: usize,
    raster_updates_deferred: bool,
    drop_target_slice_index: Option<usize>,
    raster_revision: u64,
    raster_cache: RefCell<Option<ItemViewRasterCache>>,
    fallback_icon_cache: RefCell<Option<ItemViewFallbackIconCache>>,
    virtual_refresh_state: VirtualViewRefreshState,
    thumbnail_pending: HashMap<String, thumbnails::ThumbnailKey>,
    state_cache: HashMap<PathBuf, DirectoryViewState>,
    state_cache_order: VecDeque<PathBuf>,
}

#[derive(Clone, Debug)]
struct ItemViewRasterCache {
    signature: ItemViewRasterCacheSignature,
    raster: ItemViewTileFrameRaster,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ItemViewFallbackIconImages {
    pub(crate) file: Image,
    pub(crate) folder: Image,
    pub(crate) image: Image,
    pub(crate) video: Image,
    pub(crate) audio: Image,
    pub(crate) archive: Image,
    pub(crate) pdf: Image,
    pub(crate) text: Image,
    pub(crate) code: Image,
    pub(crate) executable: Image,
}

#[derive(Clone, Debug, PartialEq)]
struct ItemViewRasterCacheSignature {
    input: ItemViewTileFrameRasterInput,
    revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ItemViewFallbackIconCacheKey {
    width: u32,
    height: u32,
    dark: bool,
    kind: i32,
}

#[derive(Clone, Debug, Default)]
struct ItemViewFallbackIconCache {
    icons: HashMap<ItemViewFallbackIconCacheKey, Image>,
    order: VecDeque<ItemViewFallbackIconCacheKey>,
}

const FALLBACK_ICON_CACHE_LIMIT: usize = 96;

impl ItemViewFallbackIconCache {
    fn icon(&mut self, width: u32, height: u32, dark: bool, kind: i32) -> Image {
        let key = ItemViewFallbackIconCacheKey {
            width,
            height,
            dark,
            kind,
        };
        if let Some(image) = self.icons.get(&key).cloned() {
            return image;
        }

        let image = render_fallback_media_icon(width, height, dark, kind);
        self.icons.insert(key, image.clone());
        self.order.push_back(key);
        while self.order.len() > FALLBACK_ICON_CACHE_LIMIT {
            if let Some(evicted) = self.order.pop_front() {
                self.icons.remove(&evicted);
            }
        }
        image
    }

    fn cached_icon_for_kind(
        &self,
        width: u32,
        height: u32,
        dark: bool,
        kind: i32,
    ) -> Option<Image> {
        let exact_key = ItemViewFallbackIconCacheKey {
            width,
            height,
            dark,
            kind,
        };
        self.icons.get(&exact_key).cloned().or_else(|| {
            self.order
                .iter()
                .rev()
                .find(|key| key.dark == dark && key.kind == kind)
                .and_then(|key| self.icons.get(key).cloned())
        })
    }
}

fn fallback_icon_kinds(kinds: &[i32]) -> Vec<i32> {
    let mut unique = Vec::with_capacity(kinds.len().max(1));
    for &kind in kinds {
        let normalized = match kind {
            ITEM_VIEW_MEDIA_KIND_FOLDER
            | ITEM_VIEW_MEDIA_KIND_IMAGE
            | ITEM_VIEW_MEDIA_KIND_VIDEO
            | ITEM_VIEW_MEDIA_KIND_AUDIO
            | ITEM_VIEW_MEDIA_KIND_ARCHIVE
            | ITEM_VIEW_MEDIA_KIND_PDF
            | ITEM_VIEW_MEDIA_KIND_TEXT
            | ITEM_VIEW_MEDIA_KIND_CODE
            | ITEM_VIEW_MEDIA_KIND_EXECUTABLE => kind,
            _ => ITEM_VIEW_MEDIA_KIND_FILE,
        };
        if !unique.contains(&normalized) {
            unique.push(normalized);
        }
    }
    if unique.is_empty() {
        unique.push(ITEM_VIEW_MEDIA_KIND_FILE);
    }
    unique
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualViewPrepareRequest {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) thumbnail_size_px: u32,
    pub(crate) schedule_thumbnails: bool,
    pub(crate) schedule_visible_thumbnail_roles_after_apply: bool,
    pub(crate) cell_width: f32,
    pub(crate) render_metrics: ItemViewRenderMetrics,
    pub(crate) show_location: bool,
    pub(crate) input: Box<VirtualViewSnapshotInput>,
}

#[derive(Clone, Debug, Default)]
struct VirtualViewRefreshState {
    in_flight: Option<u64>,
    pending: Option<VirtualViewPrepareRequest>,
}

impl VirtualViewRefreshState {
    fn has_in_flight(&self) -> bool {
        self.in_flight.is_some()
    }

    fn mark_started(&mut self, generation: u64) {
        self.in_flight = Some(generation);
    }

    fn defer(&mut self, request: VirtualViewPrepareRequest) {
        self.pending = Some(request);
    }

    fn clear_pending(&mut self) {
        self.pending = None;
    }

    fn cancel(&mut self) {
        self.in_flight = None;
        self.pending = None;
    }

    fn finish(&mut self, generation: u64) -> Option<VirtualViewPrepareRequest> {
        if self.in_flight != Some(generation) {
            return None;
        }
        self.in_flight = None;
        if let Some(pending) = self.pending.take() {
            self.in_flight = Some(pending.generation);
            return Some(pending);
        }
        None
    }
}

impl PaneView {
    pub(crate) fn invalidate_virtual_view(&mut self) {
        self.virtual_view.invalidate();
        self.raster_updates_deferred = false;
        self.virtual_generation.next();
        self.bump_raster_revision();
        self.clear_raster_cache();
        self.cancel_virtual_prepare_queue();
    }

    pub(crate) fn clear_virtual_view(&mut self) {
        self.virtual_view.clear();
        self.raster_updates_deferred = false;
        self.virtual_generation.next();
        self.bump_raster_revision();
        self.clear_raster_cache();
        self.cancel_virtual_prepare_queue();
    }

    pub(crate) fn has_virtual_prepare_in_flight(&self) -> bool {
        self.virtual_refresh_state.has_in_flight()
    }

    pub(crate) fn mark_virtual_prepare_started(&mut self, generation: u64) {
        self.virtual_refresh_state.mark_started(generation);
    }

    pub(crate) fn defer_virtual_prepare(&mut self, request: VirtualViewPrepareRequest) {
        self.virtual_refresh_state.defer(request);
    }

    pub(crate) fn clear_pending_virtual_prepare(&mut self) {
        self.virtual_refresh_state.clear_pending();
    }

    pub(crate) fn cancel_virtual_prepare_queue(&mut self) {
        self.virtual_refresh_state.cancel();
    }

    pub(crate) fn tile_frame_raster_layer(
        &self,
        mut input: ItemViewTileFrameRasterInput,
    ) -> ItemViewTileFrameRaster {
        input.drop_target_slice_index = self.drop_target_slice_index_i32();
        if !self.has_raster_layer_content() {
            self.clear_raster_cache();
            item_view_perf::log(format_args!(
                "raster empty=true width={} height={} pixels={} revision={}",
                input.width,
                input.height,
                u64::from(input.width) * u64::from(input.height),
                self.raster_revision
            ));
            return ItemViewTileFrameRaster::default();
        }

        if self.raster_updates_deferred {
            item_view_perf::log(format_args!(
                "raster deferred=true revision={}",
                self.raster_revision
            ));
            return self
                .last_raster()
                .unwrap_or_else(ItemViewTileFrameRaster::default);
        }

        let signature = ItemViewRasterCacheSignature {
            input,
            revision: self.raster_revision,
        };
        if let Some(raster) = self.cached_raster(&signature) {
            item_view_perf::log(format_args!(
                "raster cached=true width={} height={} pixels={} revision={}",
                input.width,
                input.height,
                u64::from(input.width) * u64::from(input.height),
                self.raster_revision
            ));
            return raster;
        }
        let timer = PerfTimer::start();
        let raster = ItemViewTileFrameBatch::from_bounded_entries(
            &self.virtual_entry_tokens,
            &self.virtual_bounds_entries,
        )
        .render_raster_layer(input);
        item_view_perf::log(format_args!(
            "raster cached=false width={} height={} pixels={} revision={} render_ms={:.3}",
            input.width,
            input.height,
            u64::from(input.width) * u64::from(input.height),
            self.raster_revision,
            timer.elapsed_ms()
        ));
        self.store_raster_cache(signature, raster.clone());
        raster
    }

    fn has_raster_layer_content(&self) -> bool {
        self.drop_target_slice_index.is_some()
            || self
                .virtual_entry_tokens
                .iter()
                .any(ItemViewRowToken::selected)
    }

    #[cfg(test)]
    pub(crate) fn fallback_icon_images(
        &self,
        width: u32,
        height: u32,
        dark: bool,
    ) -> ItemViewFallbackIconImages {
        self.fallback_icon_images_for_kinds(
            width,
            height,
            dark,
            &[
                ITEM_VIEW_MEDIA_KIND_FILE,
                ITEM_VIEW_MEDIA_KIND_FOLDER,
                ITEM_VIEW_MEDIA_KIND_IMAGE,
                ITEM_VIEW_MEDIA_KIND_VIDEO,
                ITEM_VIEW_MEDIA_KIND_AUDIO,
                ITEM_VIEW_MEDIA_KIND_ARCHIVE,
                ITEM_VIEW_MEDIA_KIND_PDF,
                ITEM_VIEW_MEDIA_KIND_TEXT,
                ITEM_VIEW_MEDIA_KIND_CODE,
                ITEM_VIEW_MEDIA_KIND_EXECUTABLE,
            ],
        )
    }

    pub(crate) fn fallback_icon_images_for_kinds(
        &self,
        width: u32,
        height: u32,
        dark: bool,
        kinds: &[i32],
    ) -> ItemViewFallbackIconImages {
        let width = width.max(1);
        let height = height.max(1);
        let mut icons = ItemViewFallbackIconImages::default();
        let mut cache = self.fallback_icon_cache.borrow_mut();
        let cache = cache.get_or_insert_with(ItemViewFallbackIconCache::default);
        let kinds = fallback_icon_kinds(kinds);
        let mut hits = 0;
        let mut misses = 0;
        let mut render_ms = 0.0;
        for kind in kinds.iter().copied() {
            let key = ItemViewFallbackIconCacheKey {
                width,
                height,
                dark,
                kind,
            };
            let cached = cache.icons.contains_key(&key);
            let timer = PerfTimer::start();
            let image = cache.icon(width, height, dark, kind);
            if cached {
                hits += 1;
            } else {
                misses += 1;
                render_ms += timer.elapsed_ms();
            }
            match kind {
                ITEM_VIEW_MEDIA_KIND_FOLDER => icons.folder = image,
                ITEM_VIEW_MEDIA_KIND_IMAGE => icons.image = image,
                ITEM_VIEW_MEDIA_KIND_VIDEO => icons.video = image,
                ITEM_VIEW_MEDIA_KIND_AUDIO => icons.audio = image,
                ITEM_VIEW_MEDIA_KIND_ARCHIVE => icons.archive = image,
                ITEM_VIEW_MEDIA_KIND_PDF => icons.pdf = image,
                ITEM_VIEW_MEDIA_KIND_TEXT => icons.text = image,
                ITEM_VIEW_MEDIA_KIND_CODE => icons.code = image,
                ITEM_VIEW_MEDIA_KIND_EXECUTABLE => icons.executable = image,
                _ => icons.file = image,
            }
        }
        item_view_perf::log(format_args!(
            "fallback_icons kinds={} hits={} misses={} render_ms={:.3} width={} height={} dark={}",
            kinds.len(),
            hits,
            misses,
            render_ms,
            width,
            height,
            dark
        ));
        icons
    }

    pub(crate) fn fallback_icon_images_for_kinds_with_policy(
        &self,
        width: u32,
        height: u32,
        dark: bool,
        kinds: &[i32],
    ) -> ItemViewFallbackIconImages {
        if !self.raster_updates_deferred {
            return self.fallback_icon_images_for_kinds(width, height, dark, kinds);
        }

        let width = width.max(1);
        let height = height.max(1);
        let kinds = fallback_icon_kinds(kinds);
        let mut icons = ItemViewFallbackIconImages::default();
        let cache = self.fallback_icon_cache.borrow();
        let mut hits = 0;
        let mut misses = 0;
        if let Some(cache) = cache.as_ref() {
            for kind in kinds.iter().copied() {
                let Some(image) = cache.cached_icon_for_kind(width, height, dark, kind) else {
                    misses += 1;
                    continue;
                };
                hits += 1;
                match kind {
                    ITEM_VIEW_MEDIA_KIND_FOLDER => icons.folder = image,
                    ITEM_VIEW_MEDIA_KIND_IMAGE => icons.image = image,
                    ITEM_VIEW_MEDIA_KIND_VIDEO => icons.video = image,
                    ITEM_VIEW_MEDIA_KIND_AUDIO => icons.audio = image,
                    ITEM_VIEW_MEDIA_KIND_ARCHIVE => icons.archive = image,
                    ITEM_VIEW_MEDIA_KIND_PDF => icons.pdf = image,
                    ITEM_VIEW_MEDIA_KIND_TEXT => icons.text = image,
                    ITEM_VIEW_MEDIA_KIND_CODE => icons.code = image,
                    ITEM_VIEW_MEDIA_KIND_EXECUTABLE => icons.executable = image,
                    _ => icons.file = image,
                }
            }
        } else {
            misses = kinds.len();
        }
        item_view_perf::log(format_args!(
            "fallback_icons deferred=true kinds={} hits={} misses={} width={} height={} dark={}",
            kinds.len(),
            hits,
            misses,
            width,
            height,
            dark
        ));
        icons
    }

    pub(crate) fn set_drop_target_slice_index(&mut self, slice_index: i32) -> bool {
        let next = usize::try_from(slice_index)
            .ok()
            .filter(|index| *index < self.virtual_entries.len());
        if self.drop_target_slice_index == next {
            return false;
        }
        self.drop_target_slice_index = next;
        self.bump_raster_revision();
        true
    }

    pub(crate) fn drop_target_slice_index_i32(&self) -> i32 {
        self.drop_target_slice_index
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    }

    pub(crate) fn clear_raster_cache(&self) {
        *self.raster_cache.borrow_mut() = None;
    }

    pub(crate) fn set_raster_updates_deferred(&mut self, deferred: bool) {
        self.raster_updates_deferred = deferred;
    }

    pub(crate) fn raster_updates_deferred(&self) -> bool {
        self.raster_updates_deferred
    }

    pub(crate) fn bump_raster_revision(&mut self) {
        self.raster_revision = self.raster_revision.wrapping_add(1);
    }

    fn cached_raster(
        &self,
        signature: &ItemViewRasterCacheSignature,
    ) -> Option<ItemViewTileFrameRaster> {
        let cache = self.raster_cache.borrow();
        cache
            .as_ref()
            .filter(|cache| cache.signature == *signature)
            .map(|cache| cache.raster.clone())
    }

    fn store_raster_cache(
        &self,
        signature: ItemViewRasterCacheSignature,
        raster: ItemViewTileFrameRaster,
    ) {
        *self.raster_cache.borrow_mut() = Some(ItemViewRasterCache { signature, raster });
    }

    fn last_raster(&self) -> Option<ItemViewTileFrameRaster> {
        self.raster_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.raster.clone())
    }

    #[cfg(test)]
    fn raster_cache_drop_target_slice_index(&self) -> Option<i32> {
        self.raster_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.signature.input.drop_target_slice_index)
    }

    #[cfg(test)]
    fn raster_cache_revision(&self) -> Option<u64> {
        self.raster_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.signature.revision)
    }

    #[cfg(test)]
    pub(crate) fn raster_revision_for_test(&self) -> u64 {
        self.raster_revision
    }

    pub(crate) fn has_renderable_virtual_entries(&self) -> bool {
        let row_count = self.virtual_entries.len();
        if row_count == 0 {
            return self.virtual_view.range.is_empty() && self.virtual_entry_tokens.is_empty();
        }
        let range_len = self
            .virtual_view
            .range
            .end
            .saturating_sub(self.virtual_view.range.start);
        if range_len > 0 && row_count != range_len {
            return false;
        }

        self.virtual_entry_tokens.len() == row_count
            && self
                .virtual_entry_tokens
                .iter()
                .all(ItemViewRowToken::has_renderable_title)
    }

    pub(crate) fn finish_virtual_prepare(
        &mut self,
        generation: u64,
    ) -> Option<VirtualViewPrepareRequest> {
        self.virtual_refresh_state.finish(generation)
    }

    pub(crate) fn thumbnail_pending_key(&self, path: &str) -> Option<&thumbnails::ThumbnailKey> {
        self.thumbnail_pending.get(path)
    }

    pub(crate) fn has_thumbnail_pending(&self, path: &str) -> bool {
        self.thumbnail_pending.contains_key(path)
    }

    pub(crate) fn insert_thumbnail_pending(&mut self, path: String, key: thumbnails::ThumbnailKey) {
        self.thumbnail_pending.insert(path, key);
    }

    pub(crate) fn remove_matching_thumbnail_pending(
        &mut self,
        path: &str,
        key: &thumbnails::ThumbnailKey,
    ) -> bool {
        if self
            .thumbnail_pending
            .get(path)
            .is_some_and(|pending_key| pending_key == key)
        {
            self.thumbnail_pending.remove(path);
            return true;
        }
        false
    }

    pub(crate) fn clear_thumbnail_pending(&mut self) {
        self.thumbnail_pending.clear();
    }

    pub(crate) fn cached_state(&mut self, path: &Path) -> Option<DirectoryViewState> {
        let view_state = self.state_cache.get(path).copied()?;
        self.refresh_state_cache_order(path);
        Some(view_state)
    }

    pub(crate) fn insert_state_cache(&mut self, path: PathBuf, view_state: DirectoryViewState) {
        self.state_cache.insert(path.clone(), view_state);
        self.refresh_state_cache_order(&path);
        while self.state_cache_order.len() > MAX_VIEW_STATE_CACHE_ENTRIES {
            if let Some(oldest) = self.state_cache_order.pop_front() {
                self.state_cache.remove(&oldest);
            }
        }
    }

    fn refresh_state_cache_order(&mut self, path: &Path) {
        self.state_cache_order
            .retain(|cached| cached.as_path() != path);
        self.state_cache_order.push_back(path.to_path_buf());
    }

    #[cfg(test)]
    fn state_cache_len(&self) -> usize {
        self.state_cache.len()
    }

    #[cfg(test)]
    fn state_cache_order_len(&self) -> usize {
        self.state_cache_order.len()
    }

    #[cfg(test)]
    fn contains_state_cache_path(&self, path: &Path) -> bool {
        self.state_cache.contains_key(path)
    }

    #[cfg(test)]
    fn pop_oldest_state_cache_path(&mut self) -> Option<PathBuf> {
        self.state_cache_order.pop_front()
    }

    #[cfg(test)]
    fn pop_newest_state_cache_path(&mut self) -> Option<PathBuf> {
        self.state_cache_order.pop_back()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DirectoryViewState {
    pub(crate) viewport_x: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualViewCache {
    pub(crate) range: Range<usize>,
    pub(crate) layout: Option<Arc<ItemViewLayoutEngine>>,
    pub(crate) thumbnail_size_px: u32,
}

impl Default for VirtualViewCache {
    fn default() -> Self {
        Self {
            range: 0..0,
            layout: None,
            thumbnail_size_px: 0,
        }
    }
}

impl VirtualViewCache {
    pub(crate) fn invalidate(&mut self) {
        self.range = 0..0;
    }

    pub(crate) fn clear(&mut self) {
        self.range = 0..0;
        self.layout = None;
        self.thumbnail_size_px = 0;
    }

    pub(crate) fn matches_layout_arc(
        &self,
        layout: &Arc<ItemViewLayoutEngine>,
        thumbnail_size_px: u32,
    ) -> bool {
        if self.thumbnail_size_px != thumbnail_size_px {
            return false;
        }

        self.layout.as_ref().is_some_and(|current| {
            Arc::ptr_eq(current, layout) || current.matches_layout_signature(layout.as_ref())
        })
    }

    pub(crate) fn update_layout_signature_arc(
        &mut self,
        layout: Arc<ItemViewLayoutEngine>,
        thumbnail_size_px: u32,
    ) {
        self.layout = Some(layout);
        self.thumbnail_size_px = thumbnail_size_px;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneHistory {
    back_stack: Vec<PathBuf>,
    forward_stack: Vec<PathBuf>,
}

impl PaneHistory {
    pub(crate) fn navigate_from(&mut self, previous: PathBuf, target: PathBuf) -> PaneNavigation {
        self.back_stack.push(previous.clone());
        self.forward_stack.clear();
        PaneNavigation { previous, target }
    }

    pub(crate) fn go_back_from(&mut self, previous: PathBuf) -> Option<PaneNavigation> {
        let target = self.back_stack.pop()?;
        self.forward_stack.push(previous.clone());
        Some(PaneNavigation { previous, target })
    }

    pub(crate) fn go_forward_from(&mut self, previous: PathBuf) -> Option<PaneNavigation> {
        let target = self.forward_stack.pop()?;
        self.back_stack.push(previous.clone());
        Some(PaneNavigation { previous, target })
    }

    pub(crate) fn prune_under(&mut self, mount_path: &Path) {
        self.back_stack.retain(|path| !path.starts_with(mount_path));
        self.forward_stack
            .retain(|path| !path.starts_with(mount_path));
    }

    pub(crate) fn back_len(&self) -> usize {
        self.back_stack.len()
    }

    pub(crate) fn forward_len(&self) -> usize {
        self.forward_stack.len()
    }

    #[cfg(test)]
    pub(crate) fn from_stacks(back_stack: Vec<PathBuf>, forward_stack: Vec<PathBuf>) -> Self {
        Self {
            back_stack,
            forward_stack,
        }
    }

    #[cfg(test)]
    pub(crate) fn back_paths(&self) -> &[PathBuf] {
        &self.back_stack
    }

    #[cfg(test)]
    pub(crate) fn forward_paths(&self) -> &[PathBuf] {
        &self.forward_stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::geometry::{
        ItemViewLayoutEngine, MainItemViewLayout, compact_item_view_layout,
    };
    use crate::app::model_update::update_pane_item_view_entries_model;
    use slint::Model;

    fn virtual_prepare_request(
        generation: u64,
        requested_viewport_x: f32,
    ) -> VirtualViewPrepareRequest {
        VirtualViewPrepareRequest {
            pane_id: 7,
            generation,
            thumbnail_size_px: 64,
            schedule_thumbnails: true,
            schedule_visible_thumbnail_roles_after_apply: false,
            cell_width: 100.0,
            render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(1, 1),
            show_location: false,
            input: Box::new(VirtualViewSnapshotInput {
                layout: MainItemViewLayout {
                    viewport_x: requested_viewport_x,
                    viewport_width: 250.0,
                    rows_per_column: 4,
                    cell_width: 100.0,
                    row_height: 90.0,
                    padding: 10.0,
                    item_padding: 0.0,
                    media_width: 1.0,
                    media_text_gap: 0.0,
                    title_font_size: 1.0,
                },
                requested_viewport_x,
                range_hint: None,
                thumbnail_size_px: 64,
                schedule_thumbnails: true,
                force_rebuild_model: false,
                visible_count_override: None,
                cache: VirtualViewCache::default(),
                entries: PaneEntryModel::from_entries(vec![RawFileEntry {
                    name_width_units: compact_text_width_units("item.txt"),
                    name: "item.txt".to_string(),
                    path: "/tmp/item.txt".to_string(),
                    group: String::new(),
                    location: String::new(),
                    kind: "File".to_string(),
                    size: "1 KB".to_string(),
                    size_bytes: 1024,
                    modified: "Today".to_string(),
                    modified_age_days: 0,
                    is_dir: false,
                }]),
                visible_entry_indices: None,
                visible_entries_have_locations: false,
                visible_location_groups: None,
                query: String::new(),
                kind_filter: 0,
                modified_filter: 0,
                size_filter: 0,
                chooser_patterns: Vec::new(),
            }),
        }
    }

    fn cache_for_layout(
        range: Range<usize>,
        entry_count: usize,
        thumbnail_size_px: u32,
    ) -> VirtualViewCache {
        let names = (0..entry_count)
            .map(|index| format!("item-{index}"))
            .collect::<Vec<_>>();
        let mut cache = VirtualViewCache {
            range,
            ..VirtualViewCache::default()
        };
        cache.update_layout_signature_arc(
            Arc::new(ItemViewLayoutEngine::from(compact_item_view_layout(
                250.0,
                names.iter().map(String::as_str),
                4,
                100.0,
                90.0,
                10.0,
                0.0,
                1.0,
                0.0,
                1.0,
            ))),
            thumbnail_size_px,
        );
        cache
    }

    #[test]
    fn virtual_view_cache_matches_shared_layout_by_arc_identity() {
        let layout = Arc::new(ItemViewLayoutEngine::from(compact_item_view_layout(
            250.0,
            ["alpha", "beta", "gamma", "delta"],
            4,
            100.0,
            90.0,
            10.0,
            0.0,
            1.0,
            0.0,
            1.0,
        )));
        let mut cache = VirtualViewCache {
            range: 0..4,
            ..VirtualViewCache::default()
        };
        cache.update_layout_signature_arc(Arc::clone(&layout), 64);

        assert!(cache.matches_layout_arc(&layout, 64));
        assert!(!cache.matches_layout_arc(&layout, 128));

        let same_signature = Arc::new(layout.as_ref().clone());
        assert!(cache.matches_layout_arc(&same_signature, 64));

        let source = include_str!("pane.rs");
        let body = source
            .split_once("pub(crate) fn matches_layout_arc(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn update_layout_signature_arc("))
            .map(|(body, _)| body)
            .expect("matches_layout_arc body should be present");
        assert!(body.contains("Arc::ptr_eq(current, layout)"));
    }

    #[test]
    fn virtual_view_cache_keeps_only_current_layout() {
        let first = Arc::new(ItemViewLayoutEngine::from(compact_item_view_layout(
            250.0,
            ["alpha", "beta"],
            4,
            100.0,
            90.0,
            10.0,
            0.0,
            1.0,
            0.0,
            1.0,
        )));
        let second = Arc::new(ItemViewLayoutEngine::from(compact_item_view_layout(
            250.0,
            ["alpha", "beta"],
            3,
            120.0,
            110.0,
            10.0,
            0.0,
            1.0,
            0.0,
            1.0,
        )));
        let mut cache = VirtualViewCache::default();

        cache.update_layout_signature_arc(Arc::clone(&first), 64);
        assert!(
            cache
                .layout
                .as_ref()
                .is_some_and(|layout| Arc::ptr_eq(layout, &first))
        );

        cache.update_layout_signature_arc(Arc::clone(&second), 64);
        assert!(
            cache
                .layout
                .as_ref()
                .is_some_and(|layout| Arc::ptr_eq(layout, &second))
        );

        cache.update_layout_signature_arc(Arc::clone(&first), 64);
        assert!(
            cache
                .layout
                .as_ref()
                .is_some_and(|layout| Arc::ptr_eq(layout, &first))
        );

        cache.clear();
        assert!(cache.layout.is_none());
    }

    fn test_raster_signature(
        view: &PaneView,
        mut input: ItemViewTileFrameRasterInput,
    ) -> ItemViewRasterCacheSignature {
        input.drop_target_slice_index = view.drop_target_slice_index_i32();
        ItemViewRasterCacheSignature {
            input,
            revision: view.raster_revision,
        }
    }

    #[test]
    fn pane_history_navigation_keeps_back_and_forward_independent() {
        let mut history = PaneHistory::default();
        let mut current = PathBuf::from("/home/yk");

        let nav = history.navigate_from(current.clone(), PathBuf::from("/tmp"));
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/home/yk"));
        assert_eq!(nav.target, PathBuf::from("/tmp"));
        assert_eq!(current, PathBuf::from("/tmp"));
        assert_eq!(history.back_paths(), &[PathBuf::from("/home/yk")]);
        assert!(history.forward_paths().is_empty());

        let nav = history.go_back_from(current.clone()).unwrap();
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/tmp"));
        assert_eq!(nav.target, PathBuf::from("/home/yk"));
        assert_eq!(current, PathBuf::from("/home/yk"));
        assert!(history.back_paths().is_empty());
        assert_eq!(history.forward_paths(), &[PathBuf::from("/tmp")]);

        let nav = history.go_forward_from(current.clone()).unwrap();
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/home/yk"));
        assert_eq!(nav.target, PathBuf::from("/tmp"));
        assert_eq!(current, PathBuf::from("/tmp"));
        assert_eq!(history.back_paths(), &[PathBuf::from("/home/yk")]);
        assert!(history.forward_paths().is_empty());
    }

    #[test]
    fn pane_history_prunes_removed_mount_paths() {
        let mount_path = PathBuf::from("/run/media/yk/USB");
        let mut history = PaneHistory::from_stacks(
            vec![PathBuf::from("/tmp"), mount_path.join("old")],
            vec![
                mount_path.join("future"),
                PathBuf::from("/run/media/yk/USB-sibling"),
            ],
        );

        history.prune_under(&mount_path);

        assert_eq!(history.back_paths(), &[PathBuf::from("/tmp")]);
        assert_eq!(
            history.forward_paths(),
            &[PathBuf::from("/run/media/yk/USB-sibling")]
        );
    }

    #[test]
    fn pane_selection_clear_resets_paths_and_anchor() {
        let mut selection = PaneSelection {
            paths: vec!["/tmp/a".to_string()],
            anchor: Some("/tmp/a".to_string()),
        };

        selection.clear();

        assert!(selection.paths.is_empty());
        assert!(selection.anchor.is_none());
    }

    #[test]
    fn pane_search_reset_all_clears_query_filters_and_visible_indices() {
        let mut search = PaneSearch {
            bar_open: true,
            loading: true,
            index_pending: true,
            focus_request: 3,
            query_sync_request: 2,
            query: "report".to_string(),
            recursive: false,
            kind_filter: 1,
            modified_filter: 2,
            size_filter: 3,
            visible_entry_indices: Some(Arc::from([0, 2, 4])),
            visible_entries_have_locations: true,
            visible_location_groups: None,
        };

        search.reset_all();

        assert!(!search.bar_open);
        assert!(!search.loading);
        assert!(!search.index_pending);
        assert_eq!(search.query_sync_request, 2);
        assert_eq!(search.query, "");
        assert_eq!(search.kind_filter, 0);
        assert_eq!(search.modified_filter, 0);
        assert_eq!(search.size_filter, 0);
        assert!(search.visible_entry_indices.is_none());
        assert!(!search.visible_entries_have_locations);
    }

    #[test]
    fn pane_set_entries_invalidates_visible_index_cache_without_clearing_filters() {
        let mut pane = PaneState::new(PathBuf::from("/tmp"));
        pane.search.query = "report".to_string();
        pane.search.kind_filter = 2;
        pane.search.visible_entry_indices = Some(Arc::from([0, 2, 4]));
        pane.search.visible_entries_have_locations = true;
        pane.search.visible_location_groups = Some(Arc::from(["docs".to_string()]));

        pane.set_file_entries(vec![test_entry("report.txt", "/tmp/report.txt")]);

        assert_eq!(pane.search.query, "report");
        assert_eq!(pane.search.kind_filter, 2);
        assert!(pane.search.visible_entry_indices.is_none());
        assert!(!pane.search.visible_entries_have_locations);
        assert!(pane.search.visible_location_groups.is_none());
    }

    #[test]
    fn pane_set_entries_recomputes_unfiltered_location_flag() {
        let mut pane = PaneState::new(PathBuf::from("/tmp"));
        let mut entry = test_entry("result.txt", "/tmp/docs/result.txt");
        entry.location = "docs".into();

        pane.set_file_entries(vec![entry]);

        assert!(pane.search.visible_entries_have_locations);
        assert!(pane.search.visible_entry_indices.is_none());
        assert!(pane.search.visible_location_groups.is_none());
    }

    #[test]
    fn pane_state_owns_recursive_search_runtime() {
        let mut pane = PaneState::new(PathBuf::from("/tmp"));
        let cancel = Arc::new(AtomicBool::new(false));

        pane.search_cancel = Some(cancel);
        pane.search_progress = search::SearchProgress {
            directories_scanned: 4,
            matches_found: 2,
        };
        let generation = pane.search_generation.next();

        assert!(pane.search_cancel.is_some());
        assert_eq!(pane.search_progress.directories_scanned, 4);
        assert_eq!(pane.search_progress.matches_found, 2);
        assert!(pane.search_generation.is_current(generation));
    }

    #[test]
    fn pane_state_owns_async_generations() {
        let mut pane = PaneState::new(PathBuf::from("/tmp"));

        let load = pane.load_generation.next();
        let open = pane.open_generation.next();
        let thumbnail = pane.thumbnail_generation.next();

        assert!(pane.load_generation.is_current(load));
        assert!(pane.open_generation.is_current(open));
        assert!(pane.thumbnail_generation.is_current(thumbnail));
    }

    #[test]
    fn panes_state_starts_with_active_pane() {
        let panes = PanesState::new(PathBuf::from("/tmp/active"));

        assert_eq!(panes.focused().current_dir, PathBuf::from("/tmp/active"));
        assert_eq!(panes.focused().id, 1);
        assert_eq!(panes.pane_count(), 1);
        assert!(!panes.is_split());
        assert!(panes.pane_for_slot(1).is_none());
        assert!(panes.focused().entries.is_empty());
        assert_eq!(panes.focused().history.back_len(), 0);
        assert_eq!(panes.focused().history.forward_len(), 0);
    }

    #[test]
    fn panes_state_can_open_focus_and_close_pane_slot() {
        let mut panes = PanesState::new(PathBuf::from("/tmp/left"));
        let left_id = panes.focused().id;

        assert!(panes.open_pane(PathBuf::from("/tmp/right")));
        assert!(panes.open_pane(PathBuf::from("/tmp/third")));
        let right_id = panes.pane_for_slot(1).expect("pane slot 1").id;
        let third_id = panes.pane_for_slot(2).expect("pane slot 2").id;
        assert_ne!(left_id, right_id);
        assert_ne!(right_id, third_id);
        assert!(panes.is_split());
        assert_eq!(panes.pane_count(), 3);
        assert_eq!(
            panes
                .pane_for_slot(1)
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/right"))
        );

        assert!(panes.focus_slot(1));
        assert_eq!(panes.focused().current_dir, PathBuf::from("/tmp/right"));
        assert_eq!(panes.focused().id, right_id);
        assert_eq!(
            panes
                .pane_for_slot(0)
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/left"))
        );
        assert_eq!(panes.pane_for_slot(0).expect("pane slot 0").id, left_id);

        let (closed_slot, closed) = panes
            .close_focused_pane_slot()
            .expect("focused slot 1 should close");
        assert_eq!(closed_slot, 1);
        assert_eq!(closed.current_dir, PathBuf::from("/tmp/right"));
        assert_eq!(closed.id, right_id);
        assert_eq!(panes.pane_count(), 2);
        assert!(panes.is_split());
        assert_eq!(panes.pane_for_slot(0).expect("pane slot 0").id, left_id);
        assert_eq!(panes.pane_for_slot(1).expect("pane slot 1").id, third_id);
        assert!(panes.focus_slot(1));
    }

    #[test]
    fn panes_state_closes_focused_split_pane_without_swapping_on_focus() {
        let mut panes = PanesState::new(PathBuf::from("/tmp/left"));
        let left_id = panes.focused().id;
        assert!(panes.open_pane(PathBuf::from("/tmp/right")));
        let right_id = panes.pane_for_slot(1).expect("inactive pane").id;

        assert!(panes.focus_slot(1));
        let (closed_slot, closed) = panes
            .close_focused_pane_slot()
            .expect("focused slot 1 should close");
        assert_eq!(closed_slot, 1);
        assert_eq!(closed.current_dir, PathBuf::from("/tmp/right"));
        assert_eq!(closed.id, right_id);
        assert_eq!(panes.focused().current_dir, PathBuf::from("/tmp/left"));
        assert_eq!(panes.focused().id, left_id);
        assert!(!panes.is_split());

        assert!(panes.open_pane(PathBuf::from("/tmp/next-right")));
        let next_right_id = panes.pane_for_slot(1).expect("inactive pane").id;
        assert!(panes.focus_slot(0));
        let (closed_slot, closed) = panes
            .close_focused_pane_slot()
            .expect("focused slot 0 should close");
        assert_eq!(closed_slot, 0);
        assert_eq!(closed.current_dir, PathBuf::from("/tmp/left"));
        assert_eq!(closed.id, left_id);
        assert_eq!(
            panes.focused().current_dir,
            PathBuf::from("/tmp/next-right")
        );
        assert_eq!(panes.focused().id, next_right_id);
        assert!(!panes.is_split());
    }

    #[test]
    fn pane_targets_resolve_focused_slots_and_stable_ids() {
        let mut panes = PanesState::new(PathBuf::from("/tmp/left"));
        let left_id = panes.focused().id;
        assert!(panes.open_pane(PathBuf::from("/tmp/right")));
        let right_id = panes.pane_for_slot(1).expect("inactive pane").id;

        assert_eq!(
            panes
                .pane_for_target(PaneTarget::Focused)
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/left"))
        );
        assert_eq!(
            panes
                .pane_for_target(PaneTarget::Slot(1))
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/right"))
        );
        assert_eq!(
            panes
                .pane_for_target(PaneTarget::Id(right_id))
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/right"))
        );

        panes
            .pane_mut_for_target(PaneTarget::Slot(1))
            .expect("slot 1 pane")
            .search
            .query = "right".to_string();
        assert_eq!(
            panes
                .pane_for_target(PaneTarget::Id(right_id))
                .map(|pane| pane.search.query.as_str()),
            Some("right")
        );

        assert!(panes.focus_slot(1));
        assert_eq!(
            panes
                .pane_for_target(PaneTarget::Focused)
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/right"))
        );
        assert_eq!(
            panes
                .pane_for_target(PaneTarget::Slot(1))
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/right"))
        );
        assert_eq!(
            panes
                .pane_for_target(PaneTarget::Id(left_id))
                .map(|pane| pane.current_dir.as_path()),
            Some(Path::new("/tmp/left"))
        );
        assert!(panes.pane_for_target(PaneTarget::Id(999)).is_none());
        assert!(panes.pane_mut_for_target(PaneTarget::Id(999)).is_none());
    }

    #[test]
    fn panes_state_allocates_stable_non_reused_inactive_ids() {
        let mut panes = PanesState::new(PathBuf::from("/tmp/left"));

        assert!(panes.open_pane(PathBuf::from("/tmp/right")));
        let first_inactive_id = panes.pane_for_slot(1).expect("inactive pane").id;
        assert_eq!(first_inactive_id, 2);
        assert!(panes.focus_slot(1));
        panes.close_focused_pane_slot();

        assert!(panes.open_pane(PathBuf::from("/tmp/next")));
        let second_inactive_id = panes.pane_for_slot(1).expect("inactive pane").id;
        assert_eq!(second_inactive_id, 3);
        assert_ne!(first_inactive_id, second_inactive_id);
        assert!(panes.pane_by_id(first_inactive_id).is_none());
        assert!(panes.pane_by_id(second_inactive_id).is_some());
    }

    #[test]
    fn inactive_pane_snapshot_copies_directory_view_and_search_not_history_or_selection() {
        let mut panes = PanesState::new(PathBuf::from("/tmp/active"));
        let active_id = panes.focused().id;
        panes.focused_mut().set_file_entries(vec![
            test_entry("one.txt", "/tmp/active/one.txt"),
            test_entry("two.txt", "/tmp/active/two.txt"),
        ]);
        panes.focused_mut().search = PaneSearch {
            query: "one".to_string(),
            recursive: false,
            kind_filter: 2,
            modified_filter: 1,
            size_filter: 3,
            visible_entry_indices: Some(Arc::from([0])),
            visible_entries_have_locations: true,
            visible_location_groups: None,
            ..Default::default()
        };
        panes.focused_mut().selection.paths = vec!["/tmp/active/one.txt".to_string()];
        panes.focused_mut().selection.anchor = Some("/tmp/active/one.txt".to_string());
        panes.focused_mut().history = PaneHistory::from_stacks(
            vec![PathBuf::from("/tmp/back")],
            vec![PathBuf::from("/tmp/forward")],
        );
        panes.focused_mut().view.viewport_x = 128.0;
        panes.focused_mut().view.virtual_view = cache_for_layout(4..12, 24, 80);
        panes.focused_mut().view.virtual_start_index = 4;
        let virtual_entries = panes
            .focused()
            .entries
            .iter()
            .map(ItemViewModelEntry::model_to_item_view_entry)
            .collect();
        update_pane_item_view_entries_model(
            &mut panes.focused_mut().view,
            4,
            virtual_entries,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &["/tmp/active/one.txt".to_string()],
        );

        assert!(panes.open_peer_from_focused());
        assert!(panes.open_peer_from_focused());

        let inactive = panes.pane_for_slot(1).expect("inactive pane");
        assert_ne!(inactive.id, active_id);
        assert_eq!(inactive.current_dir, PathBuf::from("/tmp/active"));
        assert_eq!(
            inactive
                .entries
                .iter()
                .map(|entry| (entry.model_name(), entry.model_path()))
                .collect::<Vec<_>>(),
            vec![
                ("one.txt", "/tmp/active/one.txt"),
                ("two.txt", "/tmp/active/two.txt")
            ]
        );
        assert_eq!(inactive.search, panes.focused().search);
        assert_eq!(inactive.view.viewport_x, 128.0);
        assert_eq!(inactive.view.virtual_view.range, 4..12);
        let inactive_layout = inactive
            .view
            .virtual_view
            .layout
            .as_ref()
            .expect("inactive pane should keep compact layout");
        let inactive_metrics = inactive_layout.layout_metrics();
        assert_eq!(inactive_metrics.entry_count, 24);
        assert_eq!(inactive_metrics.rows_per_column, 4);
        assert_eq!(inactive_metrics.cell_width, 100.0);
        assert_eq!(inactive_metrics.row_height, 90.0);
        assert_eq!(inactive.view.virtual_view.thumbnail_size_px, 80);
        assert_eq!(inactive.view.virtual_start_index, 4);
        assert_eq!(inactive.view.virtual_entries.len(), 2);
        assert_eq!(inactive.view.virtual_entry_tokens.len(), 2);
        assert!(
            inactive
                .view
                .virtual_entry_tokens
                .iter()
                .all(|token| !token.selected())
        );
        assert_eq!(
            inactive
                .view
                .virtual_entries
                .first()
                .expect("inactive row")
                .name
                .as_str(),
            "one.txt"
        );
        assert!(inactive.selection.paths.is_empty());
        assert!(inactive.selection.anchor.is_none());
        assert_eq!(inactive.history.back_len(), 0);
        assert_eq!(inactive.history.forward_len(), 0);

        panes
            .focused_mut()
            .view
            .virtual_entries
            .first_mut()
            .expect("focused row")
            .name = "changed.txt".into();
        assert_eq!(
            panes
                .pane_for_slot(1)
                .expect("inactive pane")
                .view
                .virtual_entries
                .first()
                .expect("inactive row")
                .name
                .as_str(),
            "one.txt"
        );
    }

    #[test]
    fn inactive_pane_snapshot_clears_pending_search_index_state() {
        let mut panes = PanesState::new(PathBuf::from("/tmp/active"));
        panes.focused_mut().set_file_entries(vec![
            test_entry("one.txt", "/tmp/active/one.txt"),
            test_entry("two.txt", "/tmp/active/two.txt"),
        ]);
        panes.focused_mut().search = PaneSearch {
            query: "one".to_string(),
            index_pending: true,
            visible_entry_indices: Some(Arc::from([0])),
            visible_entries_have_locations: true,
            visible_location_groups: Some(Arc::from(["docs".to_string()])),
            ..Default::default()
        };

        assert!(panes.open_peer_from_focused());

        let inactive = panes.pane_for_slot(1).expect("inactive pane");
        assert_eq!(inactive.search.query, "one");
        assert!(!inactive.search.index_pending);
        assert!(inactive.search.visible_entry_indices.is_none());
        assert!(inactive.search.visible_location_groups.is_none());
    }

    #[test]
    fn panes_state_prunes_removed_mount_from_both_panes() {
        let mount_path = PathBuf::from("/run/media/yk/USB");
        let mut panes = PanesState::new(mount_path.join("active"));
        panes.focused_mut().history = PaneHistory::from_stacks(
            vec![mount_path.join("active-old")],
            vec![mount_path.join("active-future")],
        );
        assert!(panes.open_pane(mount_path.join("inactive")));
        {
            let inactive = panes.pane_mut_for_slot(1).expect("inactive pane");
            inactive.history = PaneHistory::from_stacks(
                vec![mount_path.join("inactive-old"), PathBuf::from("/tmp/keep")],
                vec![mount_path.join("inactive-future")],
            );
        }

        assert!(panes.prune_mount_path(&mount_path, PathBuf::from("/home/yk")));

        assert_eq!(panes.focused().current_dir, PathBuf::from("/home/yk"));
        assert!(panes.focused().history.back_paths().is_empty());
        assert!(panes.focused().history.forward_paths().is_empty());
        let inactive = panes.pane_for_slot(1).expect("inactive pane");
        assert_eq!(inactive.current_dir, PathBuf::from("/home/yk"));
        assert_eq!(inactive.history.back_paths(), &[PathBuf::from("/tmp/keep")]);
        assert!(inactive.history.forward_paths().is_empty());
    }

    #[test]
    fn pane_view_virtual_cache_invalidate_keeps_metrics_but_clears_range() {
        let mut view = PaneView {
            virtual_view: cache_for_layout(4..12, 64, 128),
            ..PaneView::default()
        };

        view.virtual_view.invalidate();

        assert!(view.virtual_view.range.is_empty());
        let layout = view
            .virtual_view
            .layout
            .as_ref()
            .expect("invalidating range should keep compact layout");
        let metrics = layout.layout_metrics();
        assert_eq!(metrics.entry_count, 64);
        assert_eq!(metrics.rows_per_column, 4);
        assert_eq!(metrics.cell_width, 100.0);
        assert_eq!(metrics.row_height, 90.0);
        assert_eq!(view.virtual_view.thumbnail_size_px, 128);
    }

    #[test]
    fn pane_clear_entries_drops_directory_virtual_layout_signature() {
        let mut pane = PaneState::new(PathBuf::from("/tmp"));
        pane.view.virtual_view = cache_for_layout(4..12, 64, 128);
        pane.set_file_entries(vec![test_entry("one.txt", "/tmp/one.txt")]);
        let mut rendered = pane.entries[0].model_to_item_view_entry();
        rendered.media_token = 101;
        update_pane_item_view_entries_model(
            &mut pane.view,
            0,
            vec![rendered],
            Vec::new(),
            vec![ItemViewMediaSource {
                slice_index: 0,
                media: Image::default(),
            }],
            Vec::new(),
            &[],
        );

        pane.clear_entries();

        assert!(pane.entries.is_empty());
        assert!(pane.view.virtual_view.range.is_empty());
        assert!(pane.view.virtual_view.layout.is_none());
        assert_eq!(pane.view.virtual_view.thumbnail_size_px, 0);
        assert_eq!(pane.view.virtual_entries.len(), 0);
        assert_eq!(pane.view.virtual_item_slots.row_count(), 0);
        assert!(pane.view.virtual_slot_entries.is_empty());
    }

    #[test]
    fn pane_view_tile_raster_cache_uses_revision_signature_and_invalidates_on_view_clear() {
        let snapshot = test_entry("one.txt", "/tmp/one.txt");
        let mut view = PaneView::default();
        let initial_revision = view.raster_revision;
        update_pane_item_view_entries_model(
            &mut view,
            0,
            vec![snapshot.model_to_item_view_entry()],
            vec![ItemViewItemBounds {
                slice_index: 0,
                x: 4.0,
                y: 6.0,
                width: 40.0,
                text_width: 24.0,
            }],
            Vec::new(),
            Vec::new(),
            &[String::from("/tmp/one.txt")],
        );
        assert!(view.raster_revision > initial_revision);
        let rendered_revision = view.raster_revision;
        let input = ItemViewTileFrameRasterInput {
            width: 64,
            height: 48,
            content_origin_x: 0.0,
            drop_target_slice_index: -1,
            dark: false,
            tile_height: 20.0,
            media_x: 2.0,
            media_y: 2.0,
            media_width: 12.0,
            media_height: 12.0,
        };

        assert!(view.raster_cache_drop_target_slice_index().is_none());
        let first = view.tile_frame_raster_layer(input);
        assert_eq!(first.width, 64);
        assert_eq!(view.raster_cache_drop_target_slice_index(), Some(-1));
        assert_eq!(view.raster_cache_revision(), Some(rendered_revision));
        let current_signature = test_raster_signature(&view, input);
        assert!(view.cached_raster(&current_signature).is_some());

        assert!(view.set_drop_target_slice_index(0));
        assert!(view.raster_revision > rendered_revision);
        let drop_target_signature = test_raster_signature(&view, input);
        assert!(view.cached_raster(&drop_target_signature).is_none());
        let with_drop_target = view.tile_frame_raster_layer(input);
        assert_eq!(with_drop_target.width, 64);
        assert_eq!(view.raster_cache_drop_target_slice_index(), Some(0));
        assert_eq!(view.raster_cache_revision(), Some(view.raster_revision));
        assert!(view.cached_raster(&drop_target_signature).is_some());

        view.invalidate_virtual_view();

        assert!(view.raster_cache_drop_target_slice_index().is_none());
    }

    #[test]
    fn pane_view_can_defer_raster_rebuilds_during_icon_size_updates() {
        let snapshot = test_entry("one.txt", "/tmp/one.txt");
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            vec![snapshot.model_to_item_view_entry()],
            vec![ItemViewItemBounds {
                slice_index: 0,
                x: 4.0,
                y: 6.0,
                width: 40.0,
                text_width: 24.0,
            }],
            Vec::new(),
            Vec::new(),
            &[String::from("/tmp/one.txt")],
        );
        let input = ItemViewTileFrameRasterInput {
            width: 64,
            height: 48,
            content_origin_x: 0.0,
            drop_target_slice_index: -1,
            dark: false,
            tile_height: 20.0,
            media_x: 2.0,
            media_y: 2.0,
            media_width: 12.0,
            media_height: 12.0,
        };
        let initial = view.tile_frame_raster_layer(input);
        assert_eq!(initial.width, 64);
        assert_eq!(view.raster_cache_drop_target_slice_index(), Some(-1));
        let cached_revision = view.raster_cache_revision();

        view.set_raster_updates_deferred(true);
        assert!(view.set_drop_target_slice_index(0));
        let deferred = view.tile_frame_raster_layer(input);

        assert_eq!(deferred.width, initial.width);
        assert_eq!(view.raster_cache_drop_target_slice_index(), Some(-1));
        assert_eq!(view.raster_cache_revision(), cached_revision);

        view.set_raster_updates_deferred(false);
        let refreshed = view.tile_frame_raster_layer(input);
        assert_eq!(refreshed.width, 64);
        assert_eq!(view.raster_cache_drop_target_slice_index(), Some(0));
        assert_eq!(view.raster_cache_revision(), Some(view.raster_revision));
    }

    #[test]
    fn pane_view_skips_empty_tile_raster_without_selection_or_drop_target() {
        let snapshot = test_entry("one.txt", "/tmp/one.txt");
        let mut view = PaneView::default();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            vec![snapshot.model_to_item_view_entry()],
            vec![ItemViewItemBounds {
                slice_index: 0,
                x: 4.0,
                y: 6.0,
                width: 40.0,
                text_width: 24.0,
            }],
            Vec::new(),
            Vec::new(),
            &[],
        );
        let raster = view.tile_frame_raster_layer(ItemViewTileFrameRasterInput {
            width: 64,
            height: 48,
            content_origin_x: 0.0,
            drop_target_slice_index: -1,
            dark: false,
            tile_height: 20.0,
            media_x: 2.0,
            media_y: 2.0,
            media_width: 12.0,
            media_height: 12.0,
        });

        assert_eq!(raster.width, 1);
        assert!(view.raster_cache.borrow().is_none());
    }

    #[test]
    fn pane_view_caches_fallback_icons_by_size_and_theme_outside_tile_raster() {
        let view = PaneView::default();

        let icons = view.fallback_icon_images(32, 32, false);
        let file_buffer = icons
            .file
            .to_rgba8()
            .expect("file fallback icon should be rasterized");
        let folder_buffer = icons
            .folder
            .to_rgba8()
            .expect("folder fallback icon should be rasterized");
        let image_buffer = icons
            .image
            .to_rgba8()
            .expect("image fallback icon should be rasterized");
        let pdf_buffer = icons
            .pdf
            .to_rgba8()
            .expect("pdf fallback icon should be rasterized");
        assert_eq!(file_buffer.width(), 32);
        assert_eq!(folder_buffer.height(), 32);
        assert_ne!(file_buffer.as_slice(), folder_buffer.as_slice());
        assert_ne!(file_buffer.as_slice(), image_buffer.as_slice());
        assert_ne!(image_buffer.as_slice(), pdf_buffer.as_slice());
        assert_eq!(
            view.fallback_icon_cache
                .borrow()
                .as_ref()
                .map(|cache| cache.icons.len()),
            Some(10)
        );

        let _ = view.fallback_icon_images(32, 32, false);
        assert_eq!(
            view.fallback_icon_cache
                .borrow()
                .as_ref()
                .map(|cache| cache.icons.len()),
            Some(10)
        );
        assert!(view.raster_cache.borrow().is_none());

        let _ = view.fallback_icon_images(48, 48, true);
        assert_eq!(
            view.fallback_icon_cache
                .borrow()
                .as_ref()
                .map(|cache| cache.icons.len()),
            Some(20)
        );
        assert!(view.raster_cache.borrow().is_none());
    }

    #[test]
    fn pane_view_renders_only_requested_fallback_icon_kinds() {
        let view = PaneView::default();

        let icons = view.fallback_icon_images_for_kinds(
            32,
            32,
            false,
            &[ITEM_VIEW_MEDIA_KIND_FILE, ITEM_VIEW_MEDIA_KIND_PDF],
        );

        assert!(icons.file.to_rgba8().is_some());
        assert!(icons.pdf.to_rgba8().is_some());
        assert!(icons.folder.to_rgba8().is_none());
        assert_eq!(
            view.fallback_icon_cache
                .borrow()
                .as_ref()
                .map(|cache| cache.icons.len()),
            Some(2)
        );
    }

    #[test]
    fn pane_view_reuses_cached_fallback_icons_while_raster_updates_are_deferred() {
        let mut view = PaneView::default();

        let initial =
            view.fallback_icon_images_for_kinds(32, 32, false, &[ITEM_VIEW_MEDIA_KIND_FILE]);
        assert!(initial.file.to_rgba8().is_some());
        assert_eq!(
            view.fallback_icon_cache
                .borrow()
                .as_ref()
                .map(|cache| cache.icons.len()),
            Some(1)
        );

        view.set_raster_updates_deferred(true);
        let deferred = view.fallback_icon_images_for_kinds_with_policy(
            64,
            64,
            false,
            &[ITEM_VIEW_MEDIA_KIND_FILE, ITEM_VIEW_MEDIA_KIND_PDF],
        );
        assert!(deferred.file.to_rgba8().is_some());
        assert!(deferred.pdf.to_rgba8().is_none());
        assert_eq!(
            view.fallback_icon_cache
                .borrow()
                .as_ref()
                .map(|cache| cache.icons.len()),
            Some(1)
        );

        view.set_raster_updates_deferred(false);
        let refreshed = view.fallback_icon_images_for_kinds_with_policy(
            64,
            64,
            false,
            &[ITEM_VIEW_MEDIA_KIND_FILE, ITEM_VIEW_MEDIA_KIND_PDF],
        );
        assert!(refreshed.file.to_rgba8().is_some());
        assert!(refreshed.pdf.to_rgba8().is_some());
        assert_eq!(
            view.fallback_icon_cache
                .borrow()
                .as_ref()
                .map(|cache| cache.icons.len()),
            Some(3)
        );
    }

    #[test]
    fn pane_view_rejects_cached_rows_without_renderable_names() {
        let mut snapshot = test_entry("one.txt", "/tmp/one.txt");
        snapshot.name = String::new().into();
        let mut view = PaneView {
            virtual_view: cache_for_layout(0..1, 1, 64),
            ..PaneView::default()
        };

        update_pane_item_view_entries_model(
            &mut view,
            0,
            vec![snapshot.model_to_item_view_entry()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert!(!view.has_renderable_virtual_entries());

        let mut rendered = snapshot.model_to_item_view_entry();
        rendered.name = "one.txt".into();
        update_pane_item_view_entries_model(
            &mut view,
            0,
            vec![rendered],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &[],
        );

        assert!(view.has_renderable_virtual_entries());

        view.virtual_view.range = 0..2;

        assert!(!view.has_renderable_virtual_entries());
    }

    #[test]
    fn pane_view_virtual_prepare_queue_keeps_latest_pending_request() {
        let mut view = PaneView::default();

        view.mark_virtual_prepare_started(1);
        assert!(view.has_virtual_prepare_in_flight());
        view.defer_virtual_prepare(virtual_prepare_request(2, 100.0));
        view.defer_virtual_prepare(virtual_prepare_request(3, 300.0));

        assert!(view.finish_virtual_prepare(99).is_none());
        assert!(view.has_virtual_prepare_in_flight());

        let next = view
            .finish_virtual_prepare(1)
            .expect("latest pending request should follow the completed in-flight request");
        assert_eq!(next.generation, 3);
        assert_eq!(next.input.requested_viewport_x, 300.0);
        assert!(view.has_virtual_prepare_in_flight());

        assert!(view.finish_virtual_prepare(3).is_none());
        assert!(!view.has_virtual_prepare_in_flight());
    }

    #[test]
    fn pane_view_virtual_invalidation_clears_prepare_queue() {
        let mut view = PaneView {
            virtual_view: cache_for_layout(4..12, 64, 128),
            ..PaneView::default()
        };
        let old_generation = view.virtual_generation.current();
        view.mark_virtual_prepare_started(old_generation);
        view.defer_virtual_prepare(virtual_prepare_request(old_generation + 1, 200.0));

        view.invalidate_virtual_view();

        assert!(view.virtual_view.range.is_empty());
        assert!(!view.virtual_generation.is_current(old_generation));
        assert!(!view.has_virtual_prepare_in_flight());
        assert!(view.finish_virtual_prepare(old_generation).is_none());
    }

    #[test]
    fn pane_view_thumbnail_pending_removes_only_matching_key() {
        let mut view = PaneView::default();
        let path = "/tmp/photo.png";
        let old_key = thumbnails::fallback_key(Path::new(path), 64);
        let new_key = thumbnails::fallback_key(Path::new(path), 128);

        view.insert_thumbnail_pending(path.to_string(), new_key.clone());

        assert!(!view.remove_matching_thumbnail_pending(path, &old_key));
        assert_eq!(view.thumbnail_pending_key(path), Some(&new_key));

        assert!(view.remove_matching_thumbnail_pending(path, &new_key));
        assert!(!view.has_thumbnail_pending(path));
    }

    #[test]
    fn pane_view_state_cache_evicts_oldest_entries() {
        let mut view = PaneView::default();
        for index in 0..(MAX_VIEW_STATE_CACHE_ENTRIES + 2) {
            view.insert_state_cache(
                PathBuf::from(format!("/tmp/view-{index}")),
                DirectoryViewState {
                    viewport_x: index as f32,
                },
            );
        }

        assert_eq!(view.state_cache_len(), MAX_VIEW_STATE_CACHE_ENTRIES);
        assert_eq!(view.state_cache_order_len(), MAX_VIEW_STATE_CACHE_ENTRIES);
        assert!(!view.contains_state_cache_path(Path::new("/tmp/view-0")));
        assert!(!view.contains_state_cache_path(Path::new("/tmp/view-1")));
        assert!(view.contains_state_cache_path(Path::new("/tmp/view-2")));
    }

    #[test]
    fn pane_view_state_cache_hit_refreshes_lru_order() {
        let mut view = PaneView::default();
        let first = PathBuf::from("/tmp/first-view");
        let second = PathBuf::from("/tmp/second-view");

        view.insert_state_cache(first.clone(), DirectoryViewState { viewport_x: 10.0 });
        view.insert_state_cache(second.clone(), DirectoryViewState { viewport_x: 20.0 });

        assert_eq!(
            view.cached_state(&first).map(|state| state.viewport_x),
            Some(10.0)
        );
        assert_eq!(view.pop_newest_state_cache_path(), Some(first));
        assert_eq!(view.pop_oldest_state_cache_path(), Some(second));
    }

    fn test_entry(name: &str, path: &str) -> FileEntry {
        FileEntry {
            name: name.into(),
            path: path.into(),
            group: String::new().into(),
            location: String::new().into(),
            kind: "File".into(),
            size: "1 KB".into(),
            size_bytes: 1024.0,
            modified: "Today".into(),
            modified_age_days: 0,
            is_dir: false,
        }
    }
}
