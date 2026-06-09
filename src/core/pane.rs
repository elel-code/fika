use super::directory::{DirectoryLister, DirectoryListerEvent, LoadMode};
use super::model::{DirectoryModel, DirectoryModelSignal};
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
    selected_paths: Vec<PathBuf>,
    anchor_path: Option<PathBuf>,
    active_path: Option<PathBuf>,
}

impl SelectionState {
    pub fn selected_paths(&self) -> &[PathBuf] {
        &self.selected_paths
    }

    pub fn anchor_path(&self) -> Option<&Path> {
        self.anchor_path.as_deref()
    }

    pub fn active_path(&self) -> Option<&Path> {
        self.active_path.as_deref()
    }

    pub fn len(&self) -> usize {
        self.selected_paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.selected_paths.is_empty()
    }

    pub fn is_selected(&self, path: &Path) -> bool {
        self.selected_paths.iter().any(|selected| selected == path)
    }

    pub fn clear(&mut self) {
        self.selected_paths.clear();
        self.anchor_path = None;
        self.active_path = None;
    }

    pub fn select_only(&mut self, path: PathBuf) {
        self.selected_paths.clear();
        self.selected_paths.push(path.clone());
        self.anchor_path = Some(path);
        self.active_path = self.anchor_path.clone();
    }

    pub fn toggle(&mut self, path: PathBuf) -> bool {
        self.anchor_path = Some(path.clone());
        self.active_path = Some(path.clone());
        if let Some(index) = self
            .selected_paths
            .iter()
            .position(|selected| selected == &path)
        {
            self.selected_paths.remove(index);
            false
        } else {
            self.selected_paths.push(path);
            true
        }
    }

    pub fn replace(&mut self, paths: Vec<PathBuf>) {
        let mut seen = BTreeSet::new();
        self.selected_paths = paths
            .into_iter()
            .filter(|path| seen.insert(path.clone()))
            .collect();
        if self
            .anchor_path
            .as_ref()
            .is_none_or(|anchor| !self.selected_paths.iter().any(|path| path == anchor))
        {
            self.anchor_path = self.selected_paths.first().cloned();
        }
        if self
            .active_path
            .as_ref()
            .is_none_or(|active| !self.selected_paths.iter().any(|path| path == active))
        {
            self.active_path = self.selected_paths.first().cloned();
        }
    }

    pub fn replace_range(&mut self, anchor_path: PathBuf, paths: Vec<PathBuf>) {
        self.replace(paths);
        self.anchor_path = Some(anchor_path);
    }

    pub fn replace_range_with_active(
        &mut self,
        anchor_path: PathBuf,
        active_path: PathBuf,
        paths: Vec<PathBuf>,
    ) {
        self.replace(paths);
        self.anchor_path = Some(anchor_path);
        self.active_path = Some(active_path);
    }

    pub fn retain_existing(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let existing = paths.into_iter().collect::<BTreeSet<_>>();
        self.selected_paths.retain(|path| existing.contains(path));
        if self
            .anchor_path
            .as_ref()
            .is_some_and(|anchor| !existing.contains(anchor))
        {
            self.anchor_path = self.selected_paths.first().cloned();
        }
        if self
            .active_path
            .as_ref()
            .is_some_and(|active| !existing.contains(active))
        {
            self.active_path = self.selected_paths.first().cloned();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMove {
    Previous,
    Next,
}

#[derive(Clone, Debug, Default)]
pub struct ViewState {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub icon_size: f32,
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
}

impl PaneState {
    pub fn new(id: PaneId, current_dir: PathBuf) -> Self {
        let generation = Generation::default();
        Self {
            id,
            generation,
            current_dir: current_dir.clone(),
            model: DirectoryModel::new(),
            selection: SelectionState::default(),
            view: ViewState {
                icon_size: 48.0,
                ..ViewState::default()
            },
            lister: DirectoryLister::new(id, current_dir, generation),
            history_back: Vec::new(),
            history_forward: Vec::new(),
        }
    }

    pub fn navigate_to(&mut self, path: PathBuf, generation: Generation) {
        if self.current_dir != path {
            self.history_back.push(self.current_dir.clone());
            self.history_forward.clear();
            self.current_dir = path.clone();
            self.selection.clear();
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
        let current_dir = self.panes.get(&source)?.current_dir.clone();
        let id = self.allocator.allocate();
        self.panes.insert(id, PaneState::new(id, current_dir));
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

    pub fn selected_paths(&self, pane_id: PaneId) -> Option<&[PathBuf]> {
        self.panes
            .get(&pane_id)
            .map(|pane| pane.selection.selected_paths())
    }

    pub fn selected_count(&self, pane_id: PaneId) -> Option<usize> {
        self.panes.get(&pane_id).map(|pane| pane.selection.len())
    }

    pub fn is_selected(&self, pane_id: PaneId, path: &Path) -> bool {
        self.panes
            .get(&pane_id)
            .is_some_and(|pane| pane.selection.is_selected(path))
    }

    pub fn select_only(&mut self, pane_id: PaneId, path: PathBuf) -> bool {
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return false;
        };
        if pane.model.index_of_path(&path).is_none() {
            return false;
        }
        pane.selection.select_only(path);
        true
    }

    pub fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) -> Option<bool> {
        let pane = self.panes.get_mut(&pane_id)?;
        if pane.model.index_of_path(&path).is_none() {
            return None;
        }
        Some(pane.selection.toggle(path))
    }

    pub fn select_range_to(&mut self, pane_id: PaneId, path: PathBuf) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let target_index = pane.model.index_of_path(&path)?;
        let anchor_path = pane
            .selection
            .anchor_path()
            .and_then(|anchor| {
                pane.model
                    .index_of_path(anchor)
                    .map(|_| anchor.to_path_buf())
            })
            .unwrap_or_else(|| path.clone());
        let anchor_index = pane
            .model
            .index_of_path(&anchor_path)
            .unwrap_or(target_index);
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let paths = pane.model.entries()[start..=end]
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let count = paths.len();
        pane.selection
            .replace_range_with_active(anchor_path, path, paths);
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
            .active_path()
            .and_then(|active| pane.model.index_of_path(active))
            .or_else(|| {
                pane.selection
                    .selected_paths()
                    .last()
                    .and_then(|path| pane.model.index_of_path(path))
            });
        let target_index = match (current_index, direction) {
            (Some(index), SelectionMove::Previous) => index.saturating_sub(1),
            (Some(index), SelectionMove::Next) => (index + 1).min(pane.model.len() - 1),
            (None, SelectionMove::Previous) => pane.model.len() - 1,
            (None, SelectionMove::Next) => 0,
        };
        let target_path = pane.model.entries()[target_index].path.clone();

        if !extend {
            pane.selection.select_only(target_path);
            return Some(1);
        }

        let anchor_path = pane
            .selection
            .anchor_path()
            .and_then(|anchor| {
                pane.model
                    .index_of_path(anchor)
                    .map(|_| anchor.to_path_buf())
            })
            .unwrap_or_else(|| target_path.clone());
        let anchor_index = pane
            .model
            .index_of_path(&anchor_path)
            .unwrap_or(target_index);
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let paths = pane.model.entries()[start..=end]
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let count = paths.len();
        pane.selection
            .replace_range_with_active(anchor_path, target_path, paths);
        Some(count)
    }

    pub fn select_all(&mut self, pane_id: PaneId) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let paths = pane
            .model
            .entries()
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let count = paths.len();
        pane.selection.replace(paths);
        Some(count)
    }

    pub fn replace_selection_by_indexes(
        &mut self,
        pane_id: PaneId,
        indexes: impl IntoIterator<Item = usize>,
    ) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let paths = indexes
            .into_iter()
            .filter_map(|index| pane.model.get(index).map(|entry| entry.path.clone()))
            .collect::<Vec<_>>();
        let count = paths.len();
        pane.selection.replace(paths);
        Some(count)
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
            pane.selection
                .retain_existing(pane.model.entries().iter().map(|entry| entry.path.clone()));
        }
        Some(signals)
    }
}

#[cfg(test)]
mod tests {
    use super::super::directory::DirectoryListerEvent;
    use super::super::entries::Entry;
    use super::*;

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
            entries: vec![test_entry_at("/tmp/b", "stale.txt")],
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

        controller
            .pane_mut(first)
            .unwrap()
            .model
            .replace_listing(vec![Entry {
                name: "file.txt".to_string(),
                path: path.clone(),
                group: String::new(),
                location: String::new(),
                kind: "File".to_string(),
                size: "-".to_string(),
                size_bytes: 0,
                modified: "-".to_string(),
                modified_age_days: -1,
                is_dir: false,
            }]);
        controller
            .pane_mut(second)
            .unwrap()
            .model
            .replace_listing(vec![Entry {
                name: "file.txt".to_string(),
                path: path.clone(),
                group: String::new(),
                location: String::new(),
                kind: "File".to_string(),
                size: "-".to_string(),
                size_bytes: 0,
                modified: "-".to_string(),
                modified_age_days: -1,
                is_dir: false,
            }]);

        assert!(controller.select_only(first, path.clone()));

        assert!(controller.is_selected(first, &path));
        assert!(!controller.is_selected(second, &path));
    }

    #[test]
    fn selection_is_pruned_after_model_change() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        let keep = PathBuf::from("/tmp/a/keep.txt");
        let remove = PathBuf::from("/tmp/a/remove.txt");
        let generation = controller.pane(pane_id).unwrap().generation;

        controller
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(vec![
                Entry {
                    name: "keep.txt".to_string(),
                    path: keep.clone(),
                    group: String::new(),
                    location: String::new(),
                    kind: "File".to_string(),
                    size: "-".to_string(),
                    size_bytes: 0,
                    modified: "-".to_string(),
                    modified_age_days: -1,
                    is_dir: false,
                },
                Entry {
                    name: "remove.txt".to_string(),
                    path: remove.clone(),
                    group: String::new(),
                    location: String::new(),
                    kind: "File".to_string(),
                    size: "-".to_string(),
                    size_bytes: 0,
                    modified: "-".to_string(),
                    modified_age_days: -1,
                    is_dir: false,
                },
            ]);
        controller.select_all(pane_id);

        controller.apply_lister_event(DirectoryListerEvent::ItemsDeleted {
            pane_id,
            generation,
            request_serial: RequestSerial(1),
            path: PathBuf::from("/tmp/a"),
            paths: vec![remove.clone()],
        });

        assert_eq!(controller.selected_paths(pane_id), Some(&[keep][..]));
    }

    #[test]
    fn range_selection_uses_model_order_and_keeps_anchor() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry("c.txt"),
                test_entry("d.txt"),
            ]);

        assert!(controller.select_only(pane_id, PathBuf::from("/tmp/a/b.txt")));
        assert_eq!(
            controller.select_range_to(pane_id, PathBuf::from("/tmp/a/d.txt")),
            Some(3)
        );

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(
                &[
                    PathBuf::from("/tmp/a/b.txt"),
                    PathBuf::from("/tmp/a/c.txt"),
                    PathBuf::from("/tmp/a/d.txt")
                ][..]
            )
        );
        assert_eq!(
            controller.pane(pane_id).unwrap().selection.anchor_path(),
            Some(Path::new("/tmp/a/b.txt"))
        );
    }

    #[test]
    fn range_selection_without_anchor_starts_at_target() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(vec![test_entry("a.txt"), test_entry("b.txt")]);

        assert_eq!(
            controller.select_range_to(pane_id, PathBuf::from("/tmp/a/b.txt")),
            Some(1)
        );

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(&[PathBuf::from("/tmp/a/b.txt")][..])
        );
    }

    #[test]
    fn keyboard_selection_moves_by_model_order() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(vec![test_entry("a.txt"), test_entry("b.txt")]);

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Next, false),
            Some(1)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(&[PathBuf::from("/tmp/a/a.txt")][..])
        );

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Next, false),
            Some(1)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(&[PathBuf::from("/tmp/a/b.txt")][..])
        );

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Previous, false),
            Some(1)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(&[PathBuf::from("/tmp/a/a.txt")][..])
        );
    }

    #[test]
    fn keyboard_range_selection_keeps_anchor_and_moves_active_path() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry("c.txt"),
            ]);

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
            Some(
                &[
                    PathBuf::from("/tmp/a/a.txt"),
                    PathBuf::from("/tmp/a/b.txt"),
                    PathBuf::from("/tmp/a/c.txt")
                ][..]
            )
        );
        assert_eq!(
            controller.pane(pane_id).unwrap().selection.anchor_path(),
            Some(Path::new("/tmp/a/a.txt"))
        );
        assert_eq!(
            controller.pane(pane_id).unwrap().selection.active_path(),
            Some(Path::new("/tmp/a/c.txt"))
        );

        assert_eq!(
            controller.move_selection(pane_id, SelectionMove::Previous, true),
            Some(2)
        );
        assert_eq!(
            controller.selected_paths(pane_id),
            Some(&[PathBuf::from("/tmp/a/a.txt"), PathBuf::from("/tmp/a/b.txt")][..])
        );
    }

    #[test]
    fn rubber_band_selection_replaces_paths_by_model_indexes() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let pane_id = controller.focused().unwrap();
        controller
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(vec![
                test_entry("a.txt"),
                test_entry("b.txt"),
                test_entry("c.txt"),
            ]);

        assert_eq!(
            controller.replace_selection_by_indexes(pane_id, [0, 2, 99]),
            Some(2)
        );

        assert_eq!(
            controller.selected_paths(pane_id),
            Some(&[PathBuf::from("/tmp/a/a.txt"), PathBuf::from("/tmp/a/c.txt")][..])
        );
        assert_eq!(
            controller.pane(pane_id).unwrap().selection.anchor_path(),
            Some(Path::new("/tmp/a/a.txt"))
        );
        assert_eq!(
            controller.pane(pane_id).unwrap().selection.active_path(),
            Some(Path::new("/tmp/a/a.txt"))
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
                icon_size: 48.0,
            })
        );
        assert_eq!(
            controller.scroll_view(first, 500.0, 500.0, 200.0, 40.0),
            Some(ViewState {
                scroll_x: 200.0,
                scroll_y: 40.0,
                icon_size: 48.0,
            })
        );
        assert_eq!(
            controller.scroll_view(first, -300.0, -100.0, 200.0, 40.0),
            Some(ViewState {
                scroll_x: 0.0,
                scroll_y: 0.0,
                icon_size: 48.0,
            })
        );

        assert_eq!(controller.pane(second).unwrap().view.scroll_x, 0.0);
        assert_eq!(controller.pane(second).unwrap().view.scroll_y, 0.0);
    }

    fn test_entry(name: &str) -> Entry {
        test_entry_at("/tmp/a", name)
    }

    fn test_entry_at(parent: &str, name: &str) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from(parent).join(name),
            group: String::new(),
            location: String::new(),
            kind: "File".to_string(),
            size: "-".to_string(),
            size_bytes: 0,
            modified: "-".to_string(),
            modified_age_days: -1,
            is_dir: false,
        }
    }
}
