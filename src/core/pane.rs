use super::directory::{DirectoryLister, DirectoryListerEvent, LoadMode};
use super::entries::ItemId;
use super::model::{DirectoryModel, DirectoryModelSignal, SortDescriptor, SortOrder, SortRole};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PaneId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Generation(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestSerial(pub u64);

#[derive(Clone, Debug, Default)]
pub struct PaneGenerationCounter {
    current: u64,
}

impl PaneGenerationCounter {
    pub fn next(&mut self) -> Generation {
        self.current += 1;
        Generation(self.current)
    }

    pub fn current(&self) -> Generation {
        Generation(self.current)
    }

    pub fn is_current(&self, generation: Generation) -> bool {
        self.current == generation.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct PaneIdAllocator {
    next: u64,
}

impl PaneIdAllocator {
    pub fn allocate(&mut self) -> PaneId {
        self.next += 1;
        PaneId(self.next)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionState {
    selected_ids: Vec<ItemId>,
    excluded_ids: Vec<ItemId>,
    all_selected: bool,
    anchor_id: Option<ItemId>,
    active_id: Option<ItemId>,
    revision: u64,
}

impl SelectionState {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn selected_ids(&self) -> &[ItemId] {
        &self.selected_ids
    }

    pub fn anchor_id(&self) -> Option<ItemId> {
        self.anchor_id
    }

    pub fn active_id(&self) -> Option<ItemId> {
        self.active_id
    }

    pub fn len(&self) -> usize {
        self.selected_ids.len()
    }

    pub fn count_for_model(&self, model_len: usize) -> usize {
        if self.all_selected {
            model_len.saturating_sub(self.excluded_ids.len())
        } else {
            self.selected_ids.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.all_selected && self.selected_ids.is_empty()
    }

    pub fn is_all_selected(&self) -> bool {
        self.all_selected
    }

    pub fn is_excluded(&self, id: ItemId) -> bool {
        self.excluded_ids.iter().any(|excluded| *excluded == id)
    }

    pub fn is_selected(&self, id: ItemId) -> bool {
        if self.all_selected {
            !self.is_excluded(id)
        } else {
            self.selected_ids.iter().any(|selected| *selected == id)
        }
    }

    pub fn clear(&mut self) {
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = false;
        self.anchor_id = None;
        self.active_id = None;
        self.bump_revision();
    }

    pub fn select_only(&mut self, id: ItemId) {
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = false;
        self.selected_ids.push(id);
        self.anchor_id = Some(id);
        self.active_id = Some(id);
        self.bump_revision();
    }

    pub fn toggle(&mut self, id: ItemId) -> bool {
        if self.all_selected {
            self.anchor_id = Some(id);
            self.active_id = Some(id);
            if let Some(index) = self
                .excluded_ids
                .iter()
                .position(|excluded| *excluded == id)
            {
                self.excluded_ids.remove(index);
                self.bump_revision();
                return true;
            }
            self.excluded_ids.push(id);
            self.bump_revision();
            return false;
        }
        self.anchor_id = Some(id);
        self.active_id = Some(id);
        if let Some(index) = self
            .selected_ids
            .iter()
            .position(|selected| *selected == id)
        {
            self.selected_ids.remove(index);
            self.bump_revision();
            false
        } else {
            self.selected_ids.push(id);
            self.bump_revision();
            true
        }
    }

    pub fn replace(&mut self, ids: Vec<ItemId>) {
        self.all_selected = false;
        self.excluded_ids.clear();
        let mut seen = BTreeSet::new();
        self.selected_ids = ids
            .into_iter()
            .filter(|id| id.is_assigned() && seen.insert(*id))
            .collect();
        if self
            .anchor_id
            .is_none_or(|anchor| !self.selected_ids.iter().any(|id| *id == anchor))
        {
            self.anchor_id = self.selected_ids.first().copied();
        }
        if self
            .active_id
            .is_none_or(|active| !self.selected_ids.iter().any(|id| *id == active))
        {
            self.active_id = self.selected_ids.first().copied();
        }
        self.bump_revision();
    }

    pub fn select_all(&mut self, anchor_id: Option<ItemId>) {
        if anchor_id.is_none() {
            self.clear();
            return;
        }
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = true;
        self.anchor_id = anchor_id;
        self.active_id = anchor_id;
        self.bump_revision();
    }

    pub fn replace_range(&mut self, anchor_id: ItemId, ids: Vec<ItemId>) {
        self.replace(ids);
        self.anchor_id = Some(anchor_id);
    }

    pub fn replace_range_with_active(
        &mut self,
        anchor_id: ItemId,
        active_id: ItemId,
        ids: Vec<ItemId>,
    ) {
        self.replace(ids);
        self.anchor_id = Some(anchor_id);
        self.active_id = Some(active_id);
    }

    pub fn retain_existing_by(
        &mut self,
        mut exists: impl FnMut(ItemId) -> bool,
        fallback_id: Option<ItemId>,
    ) {
        if self.all_selected {
            let before_excluded_len = self.excluded_ids.len();
            let before_anchor = self.anchor_id;
            let before_active = self.active_id;
            self.excluded_ids.retain(|id| exists(*id));
            if self.anchor_id.is_some_and(|anchor| !exists(anchor)) {
                self.anchor_id = fallback_id;
            }
            if self.active_id.is_some_and(|active| !exists(active)) {
                self.active_id = fallback_id;
            }
            if fallback_id.is_none() {
                self.clear();
            }
            if self.excluded_ids.len() != before_excluded_len
                || self.anchor_id != before_anchor
                || self.active_id != before_active
            {
                self.bump_revision();
            }
            return;
        }

        let before_selected_len = self.selected_ids.len();
        let before_anchor = self.anchor_id;
        let before_active = self.active_id;
        self.selected_ids.retain(|id| exists(*id));
        if self.anchor_id.is_some_and(|anchor| !exists(anchor)) {
            self.anchor_id = self.selected_ids.first().copied();
        }
        if self.active_id.is_some_and(|active| !exists(active)) {
            self.active_id = self.selected_ids.first().copied();
        }
        if self.selected_ids.len() != before_selected_len
            || self.anchor_id != before_anchor
            || self.active_id != before_active
        {
            self.bump_revision();
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMove {
    Previous,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewState {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub zoom_level: i32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            viewport_width: 720.0,
            viewport_height: 520.0,
            zoom_level: DEFAULT_ZOOM_LEVEL,
        }
    }
}

pub const MIN_ZOOM_LEVEL: i32 = 0;
pub const MAX_ZOOM_LEVEL: i32 = 16;
pub const DEFAULT_ZOOM_LEVEL: i32 = 3;

pub fn icon_size_for_zoom_level(level: i32) -> f32 {
    match level.clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL) {
        0 => 16.0,
        1 => 22.0,
        2 => 32.0,
        3 => 48.0,
        4 => 64.0,
        level => 64.0 + ((level - 4) as f32 * 16.0),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoomChange {
    In,
    Out,
    Reset,
}

impl ViewState {
    pub fn reset_scroll(&mut self) {
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
    }

    pub fn icon_size(&self) -> f32 {
        icon_size_for_zoom_level(self.zoom_level)
    }

    pub fn set_zoom_level(&mut self, level: i32) -> bool {
        let level = level.clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL);
        if self.zoom_level == level {
            return false;
        }
        self.zoom_level = level;
        true
    }

    pub fn apply_zoom_change(&mut self, change: ZoomChange) -> bool {
        match change {
            ZoomChange::In => self.set_zoom_level(self.zoom_level + 1),
            ZoomChange::Out => self.set_zoom_level(self.zoom_level - 1),
            ZoomChange::Reset => self.set_zoom_level(DEFAULT_ZOOM_LEVEL),
        }
    }
}

#[derive(Debug)]
pub struct PaneState {
    pub id: PaneId,
    pub generation: Generation,
    pub current_dir: PathBuf,
    pub model: DirectoryModel,
    pub selection: SelectionState,
    pub view: ViewState,
    pub lister: DirectoryLister,
    history_back: Vec<PathBuf>,
    history_forward: Vec<PathBuf>,
    sort_order_by_role: HashMap<SortRole, SortOrder>,
}

impl PaneState {
    pub fn new(id: PaneId, current_dir: PathBuf) -> Self {
        let generation = Generation::default();
        Self {
            id,
            generation,
            current_dir: current_dir.clone(),
            model: DirectoryModel::for_directory(current_dir.clone()),
            selection: SelectionState::default(),
            view: ViewState::default(),
            lister: DirectoryLister::new(id, current_dir, generation),
            history_back: Vec::new(),
            history_forward: Vec::new(),
            sort_order_by_role: HashMap::new(),
        }
    }

    pub fn preferred_sort_order(&self, role: SortRole) -> SortOrder {
        self.sort_order_by_role
            .get(&role)
            .copied()
            .unwrap_or_else(|| role.default_order())
    }

    fn remember_sort_order(&mut self, sort: SortDescriptor) {
        self.sort_order_by_role.insert(sort.role, sort.order);
    }

    pub fn navigate_to(&mut self, path: PathBuf, generation: Generation) {
        if self.current_dir != path {
            self.history_back.push(self.current_dir.clone());
            self.history_forward.clear();
            self.current_dir = path.clone();
            self.selection.clear();
            self.view.reset_scroll();
        }
        self.generation = generation;
        self.lister.set_target(self.id, path, generation);
    }

    pub fn can_go_back(&self) -> bool {
        !self.history_back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.history_forward.is_empty()
    }

    pub fn pop_back(&mut self, generation: Generation) -> Option<PathBuf> {
        let previous = self.history_back.pop()?;
        self.history_forward.push(self.current_dir.clone());
        self.current_dir = previous.clone();
        self.selection.clear();
        self.view.reset_scroll();
        self.generation = generation;
        self.lister
            .set_target(self.id, previous.clone(), generation);
        Some(previous)
    }

    pub fn pop_forward(&mut self, generation: Generation) -> Option<PathBuf> {
        let next = self.history_forward.pop()?;
        self.history_back.push(self.current_dir.clone());
        self.current_dir = next.clone();
        self.selection.clear();
        self.view.reset_scroll();
        self.generation = generation;
        self.lister.set_target(self.id, next.clone(), generation);
        Some(next)
    }
}

#[derive(Debug)]
pub struct PaneController {
    allocator: PaneIdAllocator,
    generation_counter: PaneGenerationCounter,
    panes: HashMap<PaneId, PaneState>,
    order: Vec<PaneId>,
    focused: Option<PaneId>,
}

impl PaneController {
    pub fn new(start_dir: PathBuf) -> Self {
        let mut allocator = PaneIdAllocator::default();
        let id = allocator.allocate();
        let pane = PaneState::new(id, start_dir);
        let panes = HashMap::from([(id, pane)]);
        Self {
            allocator,
            generation_counter: PaneGenerationCounter::default(),
            panes,
            order: vec![id],
            focused: Some(id),
        }
    }

    pub fn pane_ids(&self) -> &[PaneId] {
        &self.order
    }

    pub fn focused(&self) -> Option<PaneId> {
        self.focused
    }

    pub fn focus(&mut self, pane_id: PaneId) -> bool {
        if self.panes.contains_key(&pane_id) {
            self.focused = Some(pane_id);
            true
        } else {
            false
        }
    }

    pub fn pane(&self, pane_id: PaneId) -> Option<&PaneState> {
        self.panes.get(&pane_id)
    }

    pub fn pane_mut(&mut self, pane_id: PaneId) -> Option<&mut PaneState> {
        self.panes.get_mut(&pane_id)
    }

    pub fn split(&mut self, source: PaneId) -> Option<PaneId> {
        let source_pane = self.panes.get(&source)?;
        let current_dir = source_pane.current_dir.clone();
        let generation = source_pane.generation;
        let model = source_pane.model.fork_for_pane();
        let view = source_pane.view.clone();
        let sort_order_by_role = source_pane.sort_order_by_role.clone();
        let id = self.allocator.allocate();
        let mut pane = PaneState::new(id, current_dir);
        pane.generation = generation;
        pane.lister
            .set_target(id, pane.current_dir.clone(), generation);
        pane.model = model;
        pane.view = view;
        pane.sort_order_by_role = sort_order_by_role;
        self.panes.insert(id, pane);
        let insert_at = self
            .order
            .iter()
            .position(|existing| *existing == source)
            .map_or(self.order.len(), |index| index + 1);
        self.order.insert(insert_at, id);
        self.focused = Some(id);
        Some(id)
    }

    pub fn close(&mut self, pane_id: PaneId) -> bool {
        if self.order.len() <= 1 || self.panes.remove(&pane_id).is_none() {
            return false;
        }
        self.order.retain(|id| *id != pane_id);
        if self.focused == Some(pane_id) {
            self.focused = self.order.first().copied();
        }
        true
    }

    pub fn load(&mut self, pane_id: PaneId, path: PathBuf) -> Option<DirectoryListerEvent> {
        let generation = self.generation_counter.next();
        let pane = self.panes.get_mut(&pane_id)?;
        pane.navigate_to(path, generation);
        Some(pane.lister.load_directory(LoadMode::Load))
    }

    pub fn reload(&mut self, pane_id: PaneId) -> Option<DirectoryListerEvent> {
        let pane = self.panes.get_mut(&pane_id)?;
        Some(pane.lister.load_directory(LoadMode::Reload))
    }

    pub fn can_go_back(&self, pane_id: PaneId) -> bool {
        self.panes.get(&pane_id).is_some_and(PaneState::can_go_back)
    }

    pub fn can_go_forward(&self, pane_id: PaneId) -> bool {
        self.panes
            .get(&pane_id)
            .is_some_and(PaneState::can_go_forward)
    }

    pub fn go_back(&mut self, pane_id: PaneId) -> Option<DirectoryListerEvent> {
        let generation = self.generation_counter.next();
        let pane = self.panes.get_mut(&pane_id)?;
        pane.pop_back(generation)?;
        Some(pane.lister.load_directory(LoadMode::Load))
    }

    pub fn go_forward(&mut self, pane_id: PaneId) -> Option<DirectoryListerEvent> {
        let generation = self.generation_counter.next();
        let pane = self.panes.get_mut(&pane_id)?;
        pane.pop_forward(generation)?;
        Some(pane.lister.load_directory(LoadMode::Load))
    }

    pub fn reload_panes_showing(&mut self, path: &Path) -> Vec<DirectoryListerEvent> {
        let pane_ids = self
            .order
            .iter()
            .copied()
            .filter(|pane_id| {
                self.panes
                    .get(pane_id)
                    .is_some_and(|pane| pane.current_dir == path)
            })
            .collect::<Vec<_>>();
        pane_ids
            .into_iter()
            .filter_map(|pane_id| self.reload(pane_id))
            .collect()
    }

    pub fn selected_paths(&self, pane_id: PaneId) -> Option<Vec<PathBuf>> {
        let pane = self.panes.get(&pane_id)?;
        Some(selected_paths_from_model(pane))
    }

    pub fn selection_anchor_path(&self, pane_id: PaneId) -> Option<PathBuf> {
        let pane = self.panes.get(&pane_id)?;
        path_for_selection_id(pane, pane.selection.anchor_id()?)
    }

    pub fn selection_active_path(&self, pane_id: PaneId) -> Option<PathBuf> {
        let pane = self.panes.get(&pane_id)?;
        path_for_selection_id(pane, pane.selection.active_id()?)
    }

    pub fn selected_count(&self, pane_id: PaneId) -> Option<usize> {
        self.panes
            .get(&pane_id)
            .map(|pane| pane.selection.count_for_model(pane.model.len()))
    }

    pub fn is_selected(&self, pane_id: PaneId, path: &Path) -> bool {
        self.panes.get(&pane_id).is_some_and(|pane| {
            pane.model
                .index_of_path(path)
                .map(|index| pane.selection.is_selected(pane.model.entries()[index].id))
                .unwrap_or(false)
        })
    }

    pub fn select_only(&mut self, pane_id: PaneId, path: PathBuf) -> bool {
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return false;
        };
        let Some(entry_id) = pane
            .model
            .index_of_path(&path)
            .map(|index| pane.model.entries()[index].id)
        else {
            return false;
        };
        pane.selection.select_only(entry_id);
        true
    }

    pub fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) -> Option<bool> {
        let pane = self.panes.get_mut(&pane_id)?;
        let entry_id = pane
            .model
            .index_of_path(&path)
            .map(|index| pane.model.entries()[index].id)?;
        Some(pane.selection.toggle(entry_id))
    }

    pub fn select_range_to(&mut self, pane_id: PaneId, path: PathBuf) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let target_index = pane.model.index_of_path(&path)?;
        let target_id = pane.model.entries()[target_index].id;
        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| pane.model.index_of_id(*id).is_some())
            .unwrap_or(target_id);
        let anchor_index = pane.model.index_of_id(anchor_id).unwrap_or(target_index);
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let ids = pane.model.entries()[start..=end]
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    pub fn move_selection(
        &mut self,
        pane_id: PaneId,
        direction: SelectionMove,
        extend: bool,
    ) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        if pane.model.is_empty() {
            return None;
        }

        let current_index = pane
            .selection
            .active_id()
            .and_then(|active| pane.model.index_of_id(active))
            .or_else(|| {
                pane.selection
                    .selected_ids()
                    .last()
                    .and_then(|id| pane.model.index_of_id(*id))
            });
        let target_index = match (current_index, direction) {
            (Some(index), SelectionMove::Previous) => index.saturating_sub(1),
            (Some(index), SelectionMove::Next) => (index + 1).min(pane.model.len() - 1),
            (None, SelectionMove::Previous) => pane.model.len() - 1,
            (None, SelectionMove::Next) => 0,
        };
        let target_id = pane.model.entries()[target_index].id;

        if !extend {
            pane.selection.select_only(target_id);
            return Some(1);
        }

        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| pane.model.index_of_id(*id).is_some())
            .unwrap_or(target_id);
        let anchor_index = pane.model.index_of_id(anchor_id).unwrap_or(target_index);
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let ids = pane.model.entries()[start..=end]
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    pub fn select_all(&mut self, pane_id: PaneId) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let count = pane.model.len();
        let anchor_id = pane.model.get(0).map(|entry| entry.id);
        pane.selection.select_all(anchor_id);
        Some(count)
    }

    pub fn replace_selection_by_indexes(
        &mut self,
        pane_id: PaneId,
        indexes: impl IntoIterator<Item = usize>,
    ) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let ids = indexes
            .into_iter()
            .filter_map(|index| pane.model.get(index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection.replace(ids);
        Some(count)
    }

    pub fn set_zoom_level(&mut self, pane_id: PaneId, level: i32) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.view.set_zoom_level(level);
        Some(pane.view.clone())
    }

    pub fn apply_zoom_change(&mut self, pane_id: PaneId, change: ZoomChange) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.view.apply_zoom_change(change);
        Some(pane.view.clone())
    }

    pub fn set_sort(
        &mut self,
        pane_id: PaneId,
        sort: SortDescriptor,
    ) -> Option<Vec<DirectoryModelSignal>> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.remember_sort_order(sort);
        Some(apply_pane_sort(pane, sort))
    }

    pub fn set_sort_role(
        &mut self,
        pane_id: PaneId,
        role: SortRole,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let current_sort = pane.model.sort_descriptor();
        let sort = SortDescriptor {
            role,
            order: pane.preferred_sort_order(role),
            folders_first: current_sort.folders_first,
            hidden_last: current_sort.hidden_last,
        };
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn set_sort_order(
        &mut self,
        pane_id: PaneId,
        order: SortOrder,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let mut sort = pane.model.sort_descriptor();
        sort.order = order;
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn set_sort_folders_first(
        &mut self,
        pane_id: PaneId,
        folders_first: bool,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let mut sort = pane.model.sort_descriptor();
        sort.folders_first = folders_first;
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn set_sort_hidden_last(
        &mut self,
        pane_id: PaneId,
        hidden_last: bool,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let mut sort = pane.model.sort_descriptor();
        sort.hidden_last = hidden_last;
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn preferred_sort_order(&self, pane_id: PaneId, role: SortRole) -> Option<SortOrder> {
        self.panes
            .get(&pane_id)
            .map(|pane| pane.preferred_sort_order(role))
    }

    pub fn sort_descriptor(&self, pane_id: PaneId) -> Option<SortDescriptor> {
        self.panes
            .get(&pane_id)
            .map(|pane| pane.model.sort_descriptor())
    }

    pub fn scroll_view(
        &mut self,
        pane_id: PaneId,
        delta_x: f32,
        delta_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        let next_x = (pane.view.scroll_x + delta_x).clamp(0.0, max_scroll_x.max(0.0));
        let next_y = (pane.view.scroll_y + delta_y).clamp(0.0, max_scroll_y.max(0.0));
        if next_x == pane.view.scroll_x && next_y == pane.view.scroll_y {
            return Some(pane.view.clone());
        }
        pane.view.scroll_x = next_x;
        pane.view.scroll_y = next_y;
        Some(pane.view.clone())
    }

    pub fn set_view_scroll(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.view.scroll_x = scroll_x.clamp(0.0, max_scroll_x.max(0.0));
        pane.view.scroll_y = scroll_y.clamp(0.0, max_scroll_y.max(0.0));
        Some(pane.view.clone())
    }

    pub fn set_viewport_bounds(
        &mut self,
        pane_id: PaneId,
        viewport_width: f32,
        viewport_height: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> Option<bool> {
        let pane = self.panes.get_mut(&pane_id)?;
        let viewport_width = normalize_viewport_extent(viewport_width);
        let viewport_height = normalize_viewport_extent(viewport_height);
        let scroll_x = pane.view.scroll_x.clamp(0.0, max_scroll_x.max(0.0));
        let scroll_y = pane.view.scroll_y.clamp(0.0, max_scroll_y.max(0.0));
        if viewport_value_eq(pane.view.viewport_width, viewport_width)
            && viewport_value_eq(pane.view.viewport_height, viewport_height)
            && viewport_value_eq(pane.view.scroll_x, scroll_x)
            && viewport_value_eq(pane.view.scroll_y, scroll_y)
        {
            return Some(false);
        }
        pane.view.viewport_width = viewport_width;
        pane.view.viewport_height = viewport_height;
        pane.view.scroll_x = scroll_x;
        pane.view.scroll_y = scroll_y;
        Some(true)
    }

    pub fn clear_selection(&mut self, pane_id: PaneId) -> bool {
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return false;
        };
        pane.selection.clear();
        true
    }

    pub fn apply_lister_event(
        &mut self,
        event: DirectoryListerEvent,
    ) -> Option<Vec<DirectoryModelSignal>> {
        let pane_id = event.pane_id();
        let pane = self.panes.get_mut(&pane_id)?;
        if !event.matches_target(pane.id, pane.generation, &pane.current_dir) {
            return None;
        }
        let signals = pane.lister.apply_event_to_model(event, &mut pane.model);
        if !signals.is_empty() {
            let fallback_id = pane.model.get(0).map(|entry| entry.id);
            let model = &pane.model;
            pane.selection
                .retain_existing_by(|id| model.index_of_id(id).is_some(), fallback_id);
        }
        Some(signals)
    }
}

pub fn normalize_viewport_extent(extent: f32) -> f32 {
    extent.max(1.0).floor()
}

fn viewport_value_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
}

fn apply_pane_sort(pane: &mut PaneState, sort: SortDescriptor) -> Vec<DirectoryModelSignal> {
    let signals = pane.model.set_sort(sort);
    if !signals.is_empty() {
        let fallback_id = pane.model.get(0).map(|entry| entry.id);
        let model = &pane.model;
        pane.selection
            .retain_existing_by(|id| model.index_of_id(id).is_some(), fallback_id);
        pane.view.reset_scroll();
    }
    signals
}

fn selected_paths_from_model(pane: &PaneState) -> Vec<PathBuf> {
    if pane.selection.is_all_selected() {
        return (0..pane.model.len())
            .filter(|index| {
                pane.model
                    .get(*index)
                    .is_some_and(|entry| !pane.selection.is_excluded(entry.id))
            })
            .filter_map(|index| pane.model.path_for_index(index))
            .collect();
    }

    pane.selection
        .selected_ids()
        .iter()
        .filter_map(|id| path_for_selection_id(pane, *id))
        .collect()
}

fn path_for_selection_id(pane: &PaneState, id: ItemId) -> Option<PathBuf> {
    pane.model
        .index_of_id(id)
        .and_then(|index| pane.model.path_for_index(index))
}

#[cfg(test)]
mod tests {
    use super::super::directory::DirectoryListerEvent;
    use super::super::entries::{Entry, EntryData};
    use super::*;
    use std::sync::Arc;

    #[test]
    fn split_allocates_distinct_pane_identity_for_same_path() {
        let mut controller = PaneController::new(PathBuf::from("/tmp"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();

        assert_ne!(first, second);
        assert_eq!(
            controller.pane(first).unwrap().current_dir,
            PathBuf::from("/tmp")
        );
        assert_eq!(
            controller.pane(second).unwrap().current_dir,
            PathBuf::from("/tmp")
        );
        assert_eq!(controller.focused(), Some(second));
    }

    #[test]
    fn stale_result_for_closed_pane_is_ignored() {
        let mut controller = PaneController::new(PathBuf::from("/tmp"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();
        let event = controller.reload(second).unwrap();

        assert!(controller.close(second));
        assert!(controller.apply_lister_event(event).is_none());
    }

    #[test]
    fn stale_generation_result_is_ignored() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.load(pane_id, PathBuf::from("/tmp/b"));

        let event = DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation: Generation(0),
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/b"),
            entries: Arc::new(vec![test_entry_at("/tmp/b", "stale.txt")]),
        };

        assert!(controller.apply_lister_event(event).is_none());
        assert!(controller.pane(pane_id).unwrap().model.is_empty());
    }

    #[test]
    fn same_path_split_panes_apply_their_own_lister_events() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();
        let path = PathBuf::from("/tmp/a/new.txt");

        controller.apply_lister_event(DirectoryListerEvent::ItemsAdded {
            pane_id: first,
            generation: controller.pane(first).unwrap().generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/a"),
            entries: vec![test_entry("new.txt")],
        });

        assert_eq!(
            controller.pane(first).unwrap().model.index_of_path(&path),
            Some(0)
        );
        assert!(controller.pane(second).unwrap().model.is_empty());

        controller.apply_lister_event(DirectoryListerEvent::ItemsAdded {
            pane_id: second,
            generation: controller.pane(second).unwrap().generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/a"),
            entries: vec![test_entry("new.txt")],
        });

        assert_eq!(
            controller.pane(first).unwrap().model.index_of_path(&path),
            Some(0)
        );
        assert_eq!(
            controller.pane(second).unwrap().model.index_of_path(&path),
            Some(0)
        );
    }

    #[test]
    fn manual_refresh_on_inactive_pane_targets_inactive_pane() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();
        controller.load(second, PathBuf::from("/tmp/b"));
        controller.focus(second);

        let event = controller.reload(first).unwrap();

        assert_eq!(event.pane_id(), first);
        assert_eq!(event.path(), Path::new("/tmp/a"));
        assert_eq!(controller.focused(), Some(second));
    }

    #[test]
    fn focus_never_retargets_async_result() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();
        controller.load(second, PathBuf::from("/tmp/b"));
        controller.focus(first);
        let event = DirectoryListerEvent::ListingCompleted {
            pane_id: second,
            generation: controller.pane(second).unwrap().generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/b"),
        };

        assert!(controller.apply_lister_event(event).is_some());
        assert_eq!(controller.focused(), Some(first));
    }

    #[test]
    fn loading_started_keeps_previous_model_until_listing_refresh() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        let generation = controller.pane(pane_id).unwrap().generation;
        controller.apply_lister_event(DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/a"),
            entries: Arc::new(vec![test_entry_at("/tmp/a", "old.txt")]),
        });

        let started = controller.load(pane_id, PathBuf::from("/tmp/b")).unwrap();
        let signals = controller.apply_lister_event(started.clone()).unwrap();

        assert!(signals.is_empty());
        let pane = controller.pane(pane_id).unwrap();
        assert_eq!(pane.current_dir, PathBuf::from("/tmp/b"));
        assert_eq!(pane.model.directory(), Path::new("/tmp/a"));
        assert_eq!(
            pane.model.path_for_index(0),
            Some(PathBuf::from("/tmp/a/old.txt"))
        );

        let signals = controller
            .apply_lister_event(DirectoryListerEvent::ListingRefreshed {
                pane_id,
                generation: started.generation(),
                request_serial: started.request_serial(),
                path: PathBuf::from("/tmp/b"),
                entries: Arc::new(vec![test_entry_at("/tmp/b", "new.txt")]),
            })
            .unwrap();

        assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
        let pane = controller.pane(pane_id).unwrap();
        assert_eq!(pane.model.directory(), Path::new("/tmp/b"));
        assert_eq!(
            pane.model.path_for_index(0),
            Some(PathBuf::from("/tmp/b/new.txt"))
        );
    }

    #[test]
    fn history_navigation_is_pane_scoped() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();
        controller.load(first, PathBuf::from("/tmp/a1"));
        controller.load(second, PathBuf::from("/tmp/b1"));
        controller.focus(second);

        let event = controller.go_back(first).unwrap();

        assert_eq!(event.pane_id(), first);
        assert_eq!(
            controller.pane(first).unwrap().current_dir,
            PathBuf::from("/tmp/a")
        );
        assert_eq!(
            controller.pane(second).unwrap().current_dir,
            PathBuf::from("/tmp/b1")
        );
        assert_eq!(controller.focused(), Some(second));
        assert!(controller.can_go_forward(first));
    }

    #[test]
    fn forward_navigation_uses_the_same_pane_history() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.load(pane_id, PathBuf::from("/tmp/b"));
        controller.go_back(pane_id);

        let event = controller.go_forward(pane_id).unwrap();

        assert_eq!(event.pane_id(), pane_id);
        assert_eq!(
            controller.pane(pane_id).unwrap().current_dir,
            PathBuf::from("/tmp/b")
        );
        assert!(!controller.can_go_forward(pane_id));
    }

    #[test]
    fn selection_is_scoped_to_pane_identity() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();
        let path = PathBuf::from("/tmp/a/file.txt");

        controller.pane_mut(first).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![test_entry_with_path(path.clone())]),
        );
        controller.pane_mut(second).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![test_entry_with_path(path.clone())]),
        );

        assert!(controller.select_only(first, path.clone()));

        assert!(controller.is_selected(first, &path));
        assert!(!controller.is_selected(second, &path));
    }

    #[test]
    fn split_panes_do_not_share_mutable_model_entries() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let path = PathBuf::from("/tmp/a/file.txt");
        controller.pane_mut(first).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![test_entry_with_path(path.clone())]),
        );
        let second = controller.split(first).unwrap();
        let generation = controller.pane(first).unwrap().generation;

        controller.apply_lister_event(DirectoryListerEvent::ItemsDeleted {
            pane_id: first,
            generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/a"),
            paths: vec![path.clone()],
        });

        assert!(controller.pane(first).unwrap().model.is_empty());
        assert_eq!(
            controller.pane(second).unwrap().model.index_of_path(&path),
            Some(0)
        );
    }

    #[test]
    fn selection_is_pruned_after_model_change() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        let keep = PathBuf::from("/tmp/a/keep.txt");
        let remove = PathBuf::from("/tmp/a/remove.txt");
        let generation = controller.pane(pane_id).unwrap().generation;

        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![
                test_entry_with_path(keep.clone()),
                test_entry_with_path(remove.clone()),
            ]),
        );
        controller.select_all(pane_id);

        controller.apply_lister_event(DirectoryListerEvent::ItemsDeleted {
            pane_id,
            generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/a"),
            paths: vec![remove.clone()],
        });

        assert_eq!(controller.selected_paths(pane_id), Some(vec![keep]));
    }

    #[test]
    fn select_all_keeps_selection_compact_and_toggle_excludes_item() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry("c.txt"),
            ]),
        );

        assert_eq!(controller.select_all(pane_id), Some(3));
        let selection = &controller.pane(pane_id).unwrap().selection;
        assert!(selection.is_all_selected());
        assert!(selection.selected_ids().is_empty());
        assert_eq!(controller.selected_count(pane_id), Some(3));

        assert_eq!(
            controller.toggle_selection(pane_id, PathBuf::from("/tmp/a/b.txt")),
            Some(false)
        );
        assert_eq!(controller.selected_count(pane_id), Some(2));
        assert!(!controller.is_selected(pane_id, Path::new("/tmp/a/b.txt")));
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![
                PathBuf::from("/tmp/a/a.txt"),
                PathBuf::from("/tmp/a/c.txt")
            ])
        );

        assert_eq!(
            controller.toggle_selection(pane_id, PathBuf::from("/tmp/a/b.txt")),
            Some(true)
        );
        assert_eq!(controller.selected_count(pane_id), Some(3));
    }

    #[test]
    fn selection_tracks_item_identity_across_rename_refresh() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        let generation = controller.pane(pane_id).unwrap().generation;
        let old_path = PathBuf::from("/tmp/a/old.txt");
        let new_path = PathBuf::from("/tmp/a/new.txt");

        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![test_entry("old.txt")]),
        );
        assert!(controller.select_only(pane_id, old_path.clone()));

        controller.apply_lister_event(DirectoryListerEvent::ItemsRefreshed {
            pane_id,
            generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/a"),
            pairs: vec![super::super::directory::RefreshPair {
                old_path,
                entry: Some(test_entry("new.txt")),
            }],
        });

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![new_path.clone()])
        );
        assert!(controller.is_selected(pane_id, &new_path));
    }

    #[test]
    fn range_selection_uses_model_order_and_keeps_anchor() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry("c.txt"),
                test_entry("d.txt"),
            ]),
        );

        assert!(controller.select_only(pane_id, PathBuf::from("/tmp/a/b.txt")));
        assert_eq!(
            controller.select_range_to(pane_id, PathBuf::from("/tmp/a/d.txt")),
            Some(3)
        );

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![
                PathBuf::from("/tmp/a/b.txt"),
                PathBuf::from("/tmp/a/c.txt"),
                PathBuf::from("/tmp/a/d.txt")
            ])
        );
        assert_eq!(
            controller.selection_anchor_path(pane_id),
            Some(PathBuf::from("/tmp/a/b.txt"))
        );
    }

    #[test]
    fn range_selection_without_anchor_starts_at_target() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![test_entry("a.txt"), test_entry("b.txt")]),
        );

        assert_eq!(
            controller.select_range_to(pane_id, PathBuf::from("/tmp/a/b.txt")),
            Some(1)
        );

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![PathBuf::from("/tmp/a/b.txt")])
        );
    }

    #[test]
    fn keyboard_selection_moves_by_model_order() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![test_entry("a.txt"), test_entry("b.txt")]),
        );

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Next, false),
            Some(1)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![PathBuf::from("/tmp/a/a.txt")])
        );

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Next, false),
            Some(1)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![PathBuf::from("/tmp/a/b.txt")])
        );

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Previous, false),
            Some(1)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![PathBuf::from("/tmp/a/a.txt")])
        );
    }

    #[test]
    fn keyboard_range_selection_keeps_anchor_and_moves_active_path() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry("c.txt"),
            ]),
        );

        assert!(controller.select_only(pane_id, PathBuf::from("/tmp/a/a.txt")));
        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Next, true),
            Some(2)
        );
        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Next, true),
            Some(3)
        );

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![
                PathBuf::from("/tmp/a/a.txt"),
                PathBuf::from("/tmp/a/b.txt"),
                PathBuf::from("/tmp/a/c.txt")
            ])
        );
        assert_eq!(
            controller.selection_anchor_path(pane_id),
            Some(PathBuf::from("/tmp/a/a.txt"))
        );
        assert_eq!(
            controller.selection_active_path(pane_id),
            Some(PathBuf::from("/tmp/a/c.txt"))
        );

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Previous, true),
            Some(2)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![
                PathBuf::from("/tmp/a/a.txt"),
                PathBuf::from("/tmp/a/b.txt")
            ])
        );
    }

    #[test]
    fn rubber_band_selection_replaces_paths_by_model_indexes() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller.pane_mut(pane_id).unwrap().model.replace_listing(
            PathBuf::from("/tmp/a"),
            listing(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry("c.txt"),
            ]),
        );

        assert_eq!(
            controller.replace_selection_by_indexes(pane_id, [0, 2, 99]),
            Some(2)
        );

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(vec![
                PathBuf::from("/tmp/a/a.txt"),
                PathBuf::from("/tmp/a/c.txt")
            ])
        );
        assert_eq!(
            controller.selection_anchor_path(pane_id),
            Some(PathBuf::from("/tmp/a/a.txt"))
        );
        assert_eq!(
            controller.selection_active_path(pane_id),
            Some(PathBuf::from("/tmp/a/a.txt"))
        );
    }

    #[test]
    fn compact_view_scroll_is_pane_local_and_clamped() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();

        assert_eq!(
            controller.scroll_view(first, 120.0, 30.0, 200.0, 40.0),
            Some(ViewState {
                scroll_x: 120.0,
                scroll_y: 30.0,
                ..ViewState::default()
            })
        );
        assert_eq!(
            controller.scroll_view(first, 500.0, 500.0, 200.0, 40.0),
            Some(ViewState {
                scroll_x: 200.0,
                scroll_y: 40.0,
                ..ViewState::default()
            })
        );
        assert_eq!(
            controller.scroll_view(first, -300.0, -100.0, 200.0, 40.0),
            Some(ViewState {
                scroll_x: 0.0,
                scroll_y: 0.0,
                ..ViewState::default()
            })
        );

        assert_eq!(controller.pane(second).unwrap().view.scroll_x, 0.0);
        assert_eq!(controller.pane(second).unwrap().view.scroll_y, 0.0);
    }

    #[test]
    fn compact_view_absolute_scroll_is_pane_local_and_clamped() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();

        assert_eq!(
            controller.set_view_scroll(first, 260.0, 90.0, 200.0, 40.0),
            Some(ViewState {
                scroll_x: 200.0,
                scroll_y: 40.0,
                ..ViewState::default()
            })
        );
        assert_eq!(
            controller.set_view_scroll(first, -20.0, -10.0, 200.0, 40.0),
            Some(ViewState {
                scroll_x: 0.0,
                scroll_y: 0.0,
                ..ViewState::default()
            })
        );

        assert_eq!(controller.pane(second).unwrap().view.scroll_x, 0.0);
        assert_eq!(controller.pane(second).unwrap().view.scroll_y, 0.0);
    }

    #[test]
    fn viewport_bounds_never_exceed_measured_pane_extent() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();

        assert_eq!(
            controller.set_viewport_bounds(pane_id, 320.9, 119.7, 1_000.0, 500.0),
            Some(true)
        );

        let view = &controller.pane(pane_id).unwrap().view;
        assert_eq!(view.viewport_width, 320.0);
        assert_eq!(view.viewport_height, 119.0);
    }

    #[test]
    fn navigation_resets_scroll_but_reload_preserves_it() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();

        controller.set_view_scroll(pane_id, 120.0, 30.0, 200.0, 40.0);
        controller.reload(pane_id).unwrap();
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 120.0);
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 30.0);

        controller.load(pane_id, PathBuf::from("/tmp/b")).unwrap();
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 0.0);
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 0.0);

        controller.set_view_scroll(pane_id, 80.0, 20.0, 200.0, 40.0);
        controller.go_back(pane_id).unwrap();
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 0.0);
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 0.0);

        controller.set_view_scroll(pane_id, 80.0, 20.0, 200.0, 40.0);
        controller.go_forward(pane_id).unwrap();
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 0.0);
        assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 0.0);
    }

    #[test]
    fn sort_role_uses_dolphin_default_order_and_remembers_per_role_order() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();

        assert_eq!(
            controller.preferred_sort_order(pane_id, SortRole::Name),
            Some(SortOrder::Ascending)
        );
        assert_eq!(
            controller.preferred_sort_order(pane_id, SortRole::Size),
            Some(SortOrder::Descending)
        );

        let (size_sort, _) = controller
            .set_sort_role(pane_id, SortRole::Size)
            .expect("pane exists");
        assert_eq!(
            size_sort,
            SortDescriptor {
                role: SortRole::Size,
                order: SortOrder::Descending,
                ..SortDescriptor::default()
            }
        );

        controller
            .set_sort_order(pane_id, SortOrder::Ascending)
            .expect("pane exists");
        assert_eq!(
            controller.preferred_sort_order(pane_id, SortRole::Size),
            Some(SortOrder::Ascending)
        );

        let (name_sort, _) = controller
            .set_sort_role(pane_id, SortRole::Name)
            .expect("pane exists");
        assert_eq!(
            name_sort,
            SortDescriptor {
                role: SortRole::Name,
                order: SortOrder::Ascending,
                ..SortDescriptor::default()
            }
        );

        controller
            .set_sort_order(pane_id, SortOrder::Descending)
            .expect("pane exists");
        let (size_sort, _) = controller
            .set_sort_role(pane_id, SortRole::Size)
            .expect("pane exists");
        assert_eq!(
            size_sort,
            SortDescriptor {
                role: SortRole::Size,
                order: SortOrder::Ascending,
                ..SortDescriptor::default()
            }
        );
        assert_eq!(
            controller.preferred_sort_order(pane_id, SortRole::Name),
            Some(SortOrder::Descending)
        );
    }

    #[test]
    fn split_inherits_sort_order_preferences_but_updates_are_pane_local() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();

        controller
            .set_sort_role(first, SortRole::Size)
            .expect("pane exists");
        controller
            .set_sort_order(first, SortOrder::Ascending)
            .expect("pane exists");

        let second = controller.split(first).unwrap();
        assert_eq!(
            controller.preferred_sort_order(second, SortRole::Size),
            Some(SortOrder::Ascending)
        );

        controller
            .set_sort_order(first, SortOrder::Descending)
            .expect("pane exists");

        assert_eq!(
            controller.preferred_sort_order(first, SortRole::Size),
            Some(SortOrder::Descending)
        );
        assert_eq!(
            controller.preferred_sort_order(second, SortRole::Size),
            Some(SortOrder::Ascending)
        );
    }

    #[test]
    fn sort_folder_and_hidden_toggles_are_pane_local_after_split() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();

        let second = controller.split(first).unwrap();

        let (first_sort, _) = controller
            .set_sort_folders_first(first, false)
            .expect("pane exists");
        assert!(!first_sort.folders_first);
        assert!(
            controller
                .sort_descriptor(second)
                .expect("pane exists")
                .folders_first
        );

        let (second_sort, _) = controller
            .set_sort_hidden_last(second, true)
            .expect("pane exists");
        assert!(second_sort.hidden_last);
        assert!(
            !controller
                .sort_descriptor(first)
                .expect("pane exists")
                .hidden_last
        );
    }

    #[test]
    fn zoom_level_maps_to_icon_size_and_clamps() {
        assert_eq!(icon_size_for_zoom_level(MIN_ZOOM_LEVEL - 1), 16.0);
        assert_eq!(icon_size_for_zoom_level(0), 16.0);
        assert_eq!(icon_size_for_zoom_level(1), 22.0);
        assert_eq!(icon_size_for_zoom_level(2), 32.0);
        assert_eq!(icon_size_for_zoom_level(DEFAULT_ZOOM_LEVEL), 48.0);
        assert_eq!(icon_size_for_zoom_level(4), 64.0);
        assert_eq!(icon_size_for_zoom_level(MAX_ZOOM_LEVEL), 256.0);
        assert_eq!(icon_size_for_zoom_level(MAX_ZOOM_LEVEL + 1), 256.0);
    }

    #[test]
    fn zoom_level_is_pane_local_and_split_inherits_source_view() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();

        let zoomed = controller
            .apply_zoom_change(first, ZoomChange::In)
            .expect("pane exists");
        assert_eq!(zoomed.zoom_level, DEFAULT_ZOOM_LEVEL + 1);
        assert_eq!(zoomed.icon_size(), 64.0);

        let second = controller.split(first).unwrap();
        assert_eq!(
            controller.pane(second).unwrap().view.zoom_level,
            DEFAULT_ZOOM_LEVEL + 1
        );

        let first_view = controller
            .set_zoom_level(first, MAX_ZOOM_LEVEL + 10)
            .expect("pane exists");
        assert_eq!(first_view.zoom_level, MAX_ZOOM_LEVEL);
        assert_eq!(first_view.icon_size(), 256.0);

        let second_view = controller
            .set_zoom_level(second, MIN_ZOOM_LEVEL - 10)
            .expect("pane exists");
        assert_eq!(second_view.zoom_level, MIN_ZOOM_LEVEL);
        assert_eq!(second_view.icon_size(), 16.0);
        assert_eq!(
            controller.pane(first).unwrap().view.zoom_level,
            MAX_ZOOM_LEVEL
        );

        let reset = controller
            .apply_zoom_change(second, ZoomChange::Reset)
            .expect("pane exists");
        assert_eq!(reset.zoom_level, DEFAULT_ZOOM_LEVEL);
    }

    fn test_entry(name: &str) -> Entry {
        test_entry_at("/tmp/a", name)
    }

    fn test_entry_at(parent: &str, name: &str) -> Entry {
        test_entry_with_path(PathBuf::from(parent).join(name))
    }

    fn test_entry_with_path(path: PathBuf) -> Entry {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let name_width_units = name.len() as u16;
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units,
            size_bytes: 0,
            modified_secs: None,
            mime_type: None,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    fn listing(entries: Vec<Entry>) -> Arc<Vec<Entry>> {
        Arc::new(entries)
    }
}
