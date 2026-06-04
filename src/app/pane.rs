use crate::app::item_view::{ItemViewInputState, ItemViewRenderMetrics};
use crate::app::virtual_view::VirtualViewSnapshotInput;
use crate::fs::entries::RawFileEntry;
use crate::fs::{search, thumbnails};
use crate::support::generation::GenerationCounter;
use crate::{FileEntry, ItemViewEntry};
use slint::ModelRc;
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
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
    pub(crate) entries: Arc<[PaneEntrySnapshot]>,
    pub(crate) history: PaneHistory,
    pub(crate) selection: PaneSelection,
    pub(crate) search: PaneSearch,
    pub(crate) search_cancel: Option<Arc<AtomicBool>>,
    pub(crate) search_progress: search::SearchProgress,
    pub(crate) search_generation: GenerationCounter,
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
            entries: Arc::from([]),
            history: PaneHistory::default(),
            selection: PaneSelection::default(),
            search: PaneSearch::default(),
            search_cancel: None,
            search_progress: search::SearchProgress::default(),
            search_generation: GenerationCounter::default(),
            load_generation: GenerationCounter::default(),
            open_generation: GenerationCounter::default(),
            thumbnail_generation: GenerationCounter::default(),
            view: PaneView::default(),
        }
    }

    pub(crate) fn split_snapshot(&self, id: u64) -> Self {
        let mut pane = Self::new_with_id(id, self.current_dir.clone());
        pane.set_entries(Arc::clone(&self.entries));
        pane.search = self.search.clone();
        pane.view.viewport_x = self.view.viewport_x;
        pane.view.virtual_view = self.view.virtual_view.clone();
        pane.view.virtual_start_index = self.view.virtual_start_index;
        pane.view.virtual_start_column = self.view.virtual_start_column;
        pane
    }

    pub(crate) fn set_entries(&mut self, entries: Arc<[PaneEntrySnapshot]>) {
        let has_locations = entries.iter().any(|entry| !entry.location.is_empty());
        self.set_entries_with_location_state(entries, has_locations);
    }

    pub(crate) fn set_entries_with_location_state(
        &mut self,
        entries: Arc<[PaneEntrySnapshot]>,
        has_locations: bool,
    ) {
        self.entries = entries;
        self.search.visible_entry_indices = None;
        self.search.visible_entries_have_locations = has_locations;
        self.search.visible_location_groups = None;
        self.view.invalidate_virtual_view();
    }

    pub(crate) fn clear_entries(&mut self) {
        self.entries = Arc::from([]);
        self.search.visible_entry_indices = None;
        self.search.visible_entries_have_locations = false;
        self.search.visible_location_groups = None;
        self.view.virtual_entries = ModelRc::default();
        self.view.virtual_start_index = 0;
        self.view.virtual_start_column = 0;
        self.view.invalidate_virtual_view();
    }

    pub(crate) fn entry_snapshot(&self) -> Arc<[PaneEntrySnapshot]> {
        Arc::clone(&self.entries)
    }

    #[cfg(test)]
    pub(crate) fn set_file_entries(&mut self, entries: Vec<FileEntry>) {
        self.set_entries(
            entries
                .iter()
                .map(PaneEntrySnapshot::from_entry)
                .collect::<Vec<_>>()
                .into(),
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaneEntrySnapshot {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) group: String,
    pub(crate) location: String,
    pub(crate) kind: String,
    pub(crate) size: String,
    pub(crate) size_bytes: f32,
    pub(crate) modified: String,
    pub(crate) modified_age_days: i32,
    pub(crate) is_dir: bool,
}

impl PaneEntrySnapshot {
    pub(crate) fn from_entry(entry: &FileEntry) -> Self {
        Self {
            name: entry.name.to_string(),
            path: entry.path.to_string(),
            group: entry.group.to_string(),
            location: entry.location.to_string(),
            kind: entry.kind.to_string(),
            size: entry.size.to_string(),
            size_bytes: entry.size_bytes,
            modified: entry.modified.to_string(),
            modified_age_days: entry.modified_age_days,
            is_dir: entry.is_dir,
        }
    }

    pub(crate) fn from_raw(entry: RawFileEntry) -> Self {
        Self {
            name: entry.name,
            path: entry.path,
            group: entry.group,
            location: entry.location,
            kind: entry.kind,
            size: entry.size,
            size_bytes: entry.size_bytes as f32,
            modified: entry.modified,
            modified_age_days: entry.modified_age_days,
            is_dir: entry.is_dir,
        }
    }

    pub(crate) fn to_file_entry(&self) -> FileEntry {
        FileEntry {
            name: self.name.as_str().into(),
            path: self.path.as_str().into(),
            group: self.group.as_str().into(),
            location: self.location.as_str().into(),
            kind: self.kind.as_str().into(),
            size: self.size.as_str().into(),
            size_bytes: self.size_bytes,
            modified: self.modified.as_str().into(),
            modified_age_days: self.modified_age_days,
            is_dir: self.is_dir,
        }
    }

    pub(crate) fn to_item_view_entry(&self) -> ItemViewEntry {
        ItemViewEntry {
            name: self.name.as_str().into(),
            path: self.path.as_str().into(),
            group: self.group.as_str().into(),
            location: self.location.as_str().into(),
            is_dir: self.is_dir,
            selected: false,
            thumbnail_state: 0,
            media: Default::default(),
            media_token: 0,
            tile_width: 0.0,
            tile_height: 0.0,
            media_x: 0.0,
            media_y: 0.0,
            text_x: 0.0,
            text_width: 0.0,
            group_y: 0.0,
            title_y: 0.0,
            location_y: 0.0,
            metadata_line_height: 0.0,
            title_line_height: 0.0,
            media_width: 0.0,
            media_height: 0.0,
            metadata_font_size: 0.0,
            title_font_size: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PreparedDirectoryEntries {
    pub(crate) entries: Arc<[PaneEntrySnapshot]>,
    pub(crate) has_locations: bool,
}

impl PreparedDirectoryEntries {
    pub(crate) fn new(entries: Vec<PaneEntrySnapshot>) -> Self {
        let has_locations = entries.iter().any(|entry| !entry.location.is_empty());
        Self {
            entries: entries.into(),
            has_locations,
        }
    }

    pub(crate) fn from_raw_entries(entries: Vec<RawFileEntry>) -> Self {
        Self::new(
            entries
                .into_iter()
                .map(PaneEntrySnapshot::from_raw)
                .collect(),
        )
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
    pub(crate) query: String,
    pub(crate) recursive: bool,
    pub(crate) kind_filter: i32,
    pub(crate) modified_filter: i32,
    pub(crate) size_filter: i32,
    pub(crate) visible_entry_indices: Option<Vec<usize>>,
    pub(crate) visible_entries_have_locations: bool,
    pub(crate) visible_location_groups: Option<Vec<String>>,
}

impl PaneSearch {
    pub(crate) fn reset_all(&mut self) {
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
    pub(crate) virtual_entries: ModelRc<ItemViewEntry>,
    pub(crate) virtual_start_index: usize,
    pub(crate) virtual_start_column: usize,
    virtual_prepare_in_flight: Option<u64>,
    virtual_prepare_pending: Option<VirtualViewPrepareRequest>,
    thumbnail_pending: HashMap<String, thumbnails::ThumbnailKey>,
    state_cache: HashMap<PathBuf, DirectoryViewState>,
    state_cache_order: VecDeque<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualViewPrepareRequest {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) thumbnail_size_px: u32,
    pub(crate) schedule_thumbnails: bool,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) row_height: f32,
    pub(crate) render_metrics: ItemViewRenderMetrics,
    pub(crate) input: Box<VirtualViewSnapshotInput>,
}

impl PaneView {
    pub(crate) fn invalidate_virtual_view(&mut self) {
        self.virtual_view.invalidate();
        self.virtual_generation.next();
        self.cancel_virtual_prepare_queue();
    }

    pub(crate) fn has_virtual_prepare_in_flight(&self) -> bool {
        self.virtual_prepare_in_flight.is_some()
    }

    pub(crate) fn mark_virtual_prepare_started(&mut self, generation: u64) {
        self.virtual_prepare_in_flight = Some(generation);
    }

    pub(crate) fn defer_virtual_prepare(&mut self, request: VirtualViewPrepareRequest) {
        self.virtual_prepare_pending = Some(request);
    }

    pub(crate) fn clear_pending_virtual_prepare(&mut self) {
        self.virtual_prepare_pending = None;
    }

    pub(crate) fn cancel_virtual_prepare_queue(&mut self) {
        self.virtual_prepare_in_flight = None;
        self.virtual_prepare_pending = None;
    }

    pub(crate) fn finish_virtual_prepare(
        &mut self,
        generation: u64,
    ) -> Option<VirtualViewPrepareRequest> {
        if self.virtual_prepare_in_flight != Some(generation) {
            return None;
        }
        self.virtual_prepare_in_flight = None;
        if let Some(pending) = self.virtual_prepare_pending.take() {
            self.virtual_prepare_in_flight = Some(pending.generation);
            return Some(pending);
        }
        None
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
    pub(crate) entry_count: usize,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) row_height: f32,
    pub(crate) thumbnail_size_px: u32,
}

impl Default for VirtualViewCache {
    fn default() -> Self {
        Self {
            range: 0..0,
            entry_count: 0,
            rows_per_column: 0,
            cell_width: 0.0,
            row_height: 0.0,
            thumbnail_size_px: 0,
        }
    }
}

impl VirtualViewCache {
    pub(crate) fn invalidate(&mut self) {
        self.range = 0..0;
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
    use crate::app::geometry::MainGridLayout;

    fn virtual_prepare_request(
        generation: u64,
        requested_viewport_x: f32,
    ) -> VirtualViewPrepareRequest {
        VirtualViewPrepareRequest {
            pane_id: 7,
            generation,
            thumbnail_size_px: 64,
            schedule_thumbnails: true,
            rows_per_column: 4,
            cell_width: 100.0,
            row_height: 90.0,
            render_metrics: ItemViewRenderMetrics::from_zoom_level(1),
            input: Box::new(VirtualViewSnapshotInput {
                layout: MainGridLayout {
                    viewport_x: requested_viewport_x,
                    rows_per_column: 4,
                    cell_width: 100.0,
                    row_height: 90.0,
                    padding: 10.0,
                },
                requested_viewport_x,
                viewport_width: 250.0,
                thumbnail_size_px: 64,
                schedule_thumbnails: true,
                visible_count_override: None,
                cache: VirtualViewCache::default(),
                entries: Arc::from([PaneEntrySnapshot {
                    name: "item.txt".to_string(),
                    path: "/tmp/item.txt".to_string(),
                    group: String::new(),
                    location: String::new(),
                    kind: "File".to_string(),
                    size: "1 KB".to_string(),
                    size_bytes: 1024.0,
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
            query: "report".to_string(),
            recursive: false,
            kind_filter: 1,
            modified_filter: 2,
            size_filter: 3,
            visible_entry_indices: Some(vec![0, 2, 4]),
            visible_entries_have_locations: true,
            visible_location_groups: None,
        };

        search.reset_all();

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
        pane.search.visible_entry_indices = Some(vec![0, 2, 4]);
        pane.search.visible_entries_have_locations = true;
        pane.search.visible_location_groups = Some(vec!["docs".to_string()]);

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
            visible_entry_indices: Some(vec![0]),
            visible_entries_have_locations: true,
            visible_location_groups: None,
        };
        panes.focused_mut().selection.paths = vec!["/tmp/active/one.txt".to_string()];
        panes.focused_mut().selection.anchor = Some("/tmp/active/one.txt".to_string());
        panes.focused_mut().history = PaneHistory::from_stacks(
            vec![PathBuf::from("/tmp/back")],
            vec![PathBuf::from("/tmp/forward")],
        );
        panes.focused_mut().view.viewport_x = 128.0;
        panes.focused_mut().view.virtual_view = VirtualViewCache {
            range: 4..12,
            entry_count: 24,
            rows_per_column: 4,
            cell_width: 208.0,
            row_height: 90.0,
            thumbnail_size_px: 80,
        };
        panes.focused_mut().view.virtual_start_index = 4;
        panes.focused_mut().view.virtual_start_column = 1;

        assert!(panes.open_peer_from_focused());
        assert!(panes.open_peer_from_focused());

        let inactive = panes.pane_for_slot(1).expect("inactive pane");
        assert_ne!(inactive.id, active_id);
        assert_eq!(inactive.current_dir, PathBuf::from("/tmp/active"));
        assert_eq!(
            inactive
                .entries
                .iter()
                .map(|entry| (entry.name.as_str(), entry.path.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("one.txt", "/tmp/active/one.txt"),
                ("two.txt", "/tmp/active/two.txt")
            ]
        );
        assert_eq!(inactive.search, panes.focused().search);
        assert_eq!(inactive.view.viewport_x, 128.0);
        assert_eq!(inactive.view.virtual_view.range, 4..12);
        assert_eq!(inactive.view.virtual_view.entry_count, 24);
        assert_eq!(inactive.view.virtual_view.rows_per_column, 4);
        assert_eq!(inactive.view.virtual_view.cell_width, 208.0);
        assert_eq!(inactive.view.virtual_view.row_height, 90.0);
        assert_eq!(inactive.view.virtual_view.thumbnail_size_px, 80);
        assert_eq!(inactive.view.virtual_start_index, 4);
        assert_eq!(inactive.view.virtual_start_column, 1);
        assert!(inactive.selection.paths.is_empty());
        assert!(inactive.selection.anchor.is_none());
        assert_eq!(inactive.history.back_len(), 0);
        assert_eq!(inactive.history.forward_len(), 0);
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
            virtual_view: VirtualViewCache {
                range: 4..12,
                entry_count: 64,
                rows_per_column: 8,
                cell_width: 96.0,
                row_height: 78.0,
                thumbnail_size_px: 128,
            },
            ..PaneView::default()
        };

        view.virtual_view.invalidate();

        assert!(view.virtual_view.range.is_empty());
        assert_eq!(view.virtual_view.entry_count, 64);
        assert_eq!(view.virtual_view.rows_per_column, 8);
        assert_eq!(view.virtual_view.cell_width, 96.0);
        assert_eq!(view.virtual_view.row_height, 78.0);
        assert_eq!(view.virtual_view.thumbnail_size_px, 128);
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
            virtual_view: VirtualViewCache {
                range: 4..12,
                entry_count: 64,
                rows_per_column: 8,
                cell_width: 96.0,
                row_height: 78.0,
                thumbnail_size_px: 128,
            },
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
