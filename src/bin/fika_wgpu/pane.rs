use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use fika_core::{Entry, ItemLayout, ViewRect, ViewSize, read_entries_sync};

use crate::{
    filtered_indexes_for_entries, wgpu_options::ShellViewMode, wgpu_selection::ShellSelection,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellPaneId {
    First,
    Second,
}

impl ShellPaneId {
    pub(crate) const FIRST: Self = Self::First;
    pub(crate) const SECOND: Self = Self::Second;
    pub(crate) const ALL: [Self; 2] = [Self::FIRST, Self::SECOND];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::First => "pane-0",
            Self::Second => "pane-1",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneGeometry {
    pub(crate) kind: ShellPaneId,
    pub(crate) pane: ViewRect,
    pub(crate) top_bar: ViewRect,
    pub(crate) content: ViewRect,
    pub(crate) status_bar: ViewRect,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellPaneState {
    pub(crate) path: PathBuf,
    pub(crate) view_mode: ShellViewMode,
    pub(crate) entries: Vec<Entry>,
    pub(crate) dir_count: usize,
    pub(crate) filtered_indexes: Vec<usize>,
    pub(crate) selection: ShellSelection,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
}

impl ShellPaneState {
    pub(crate) fn from_entries(
        path: PathBuf,
        view_mode: ShellViewMode,
        entries: Vec<Entry>,
        show_hidden: bool,
        filter_pattern: &str,
    ) -> Self {
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let filtered_indexes = filtered_indexes_for_entries(&entries, show_hidden, filter_pattern);
        Self {
            path,
            view_mode,
            entries,
            dir_count,
            filtered_indexes,
            selection: ShellSelection::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }

    pub(crate) fn load(
        path: PathBuf,
        view_mode: ShellViewMode,
        show_hidden: bool,
    ) -> Result<Self, String> {
        let load_start = Instant::now();
        let entries = read_entries_sync(&path)
            .map_err(|error| format!("read pane directory {}: {error}", path.display()))?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let filtered_indexes = filtered_indexes_for_entries(&entries, show_hidden, "");
        eprintln!(
            "[fika-wgpu] split-pane path={} entries={} dirs={} files={} visible={} load={}us",
            path.display(),
            entries.len(),
            dir_count,
            entries.len().saturating_sub(dir_count),
            filtered_indexes.len(),
            elapsed.as_micros()
        );
        Ok(Self {
            path,
            view_mode,
            entries,
            dir_count,
            filtered_indexes,
            selection: ShellSelection::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        })
    }

    #[cfg(test)]
    pub(crate) fn filtered_entry_count(&self) -> usize {
        self.filtered_indexes.len()
    }

    pub(crate) fn rebuild_filtered_indexes(&mut self, show_hidden: bool) -> bool {
        self.filtered_indexes = filtered_indexes_for_entries(&self.entries, show_hidden, "");
        self.selection.retain_indexes(&self.filtered_indexes)
    }

    pub(crate) fn rebuild_filtered_indexes_with_pattern(
        &mut self,
        show_hidden: bool,
        filter_pattern: &str,
    ) -> bool {
        self.filtered_indexes =
            filtered_indexes_for_entries(&self.entries, show_hidden, filter_pattern);
        self.selection.retain_indexes(&self.filtered_indexes)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShellPaneView<'a> {
    pub(crate) path: &'a Path,
    pub(crate) view_mode: ShellViewMode,
    pub(crate) entries: &'a [Entry],
    pub(crate) dir_count: usize,
    pub(crate) filtered_indexes: &'a [usize],
    pub(crate) selection: &'a ShellSelection,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
}

impl<'a> ShellPaneView<'a> {
    pub(crate) fn from_state(state: &'a ShellPaneState) -> Self {
        Self {
            path: &state.path,
            view_mode: state.view_mode,
            entries: &state.entries,
            dir_count: state.dir_count,
            filtered_indexes: &state.filtered_indexes,
            selection: &state.selection,
            scroll_x: state.scroll_x,
            scroll_y: state.scroll_y,
        }
    }

    pub(crate) fn filtered_entry_count(self) -> usize {
        self.filtered_indexes.len()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellPaneProjection<'a> {
    pub(crate) view: ShellPaneView<'a>,
    pub(crate) geometry: ShellPaneGeometry,
    pub(crate) visible_items: Vec<ShellPaneVisibleItem>,
    pub(crate) scroll_metrics: ShellPaneScrollMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneVisibleItem {
    pub(crate) layout: ItemLayout,
    pub(crate) slot_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneScrollMetrics {
    pub(crate) content_size: ViewSize,
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) max_scroll_x: f32,
    pub(crate) max_scroll_y: f32,
}

impl ShellPaneScrollMetrics {
    pub(crate) fn new(content_size: ViewSize, viewport: ViewRect) -> Self {
        let viewport_width = viewport.width.max(1.0);
        let viewport_height = viewport.height.max(1.0);
        Self {
            content_size,
            viewport_width,
            viewport_height,
            max_scroll_x: (content_size.width - viewport_width).max(0.0),
            max_scroll_y: (content_size.height - viewport_height).max(0.0),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellVisibleItemSlot {
    slot_id: u64,
    visible_epoch: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellVisibleItemSlotStats {
    pub(crate) active: usize,
    pub(crate) free: usize,
    pub(crate) reused: usize,
    pub(crate) recycled: usize,
    pub(crate) allocated: usize,
}

impl ShellVisibleItemSlotStats {
    pub(crate) fn merged(self, other: Self) -> Self {
        Self {
            active: self.active + other.active,
            free: self.free + other.free,
            reused: self.reused + other.reused,
            recycled: self.recycled + other.recycled,
            allocated: self.allocated + other.allocated,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShellVisibleItemSlotPool {
    next_slot_id: u64,
    visible_epoch: u64,
    slot_by_path: HashMap<PathBuf, ShellVisibleItemSlot>,
    free_slots: Vec<u64>,
}

impl ShellVisibleItemSlotPool {
    const MAX_FREE_SLOTS: usize = 100;

    pub(crate) fn update_visible_items(
        &mut self,
        visible_paths: impl IntoIterator<Item = PathBuf>,
    ) -> ShellVisibleItemSlotStats {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let visible_epoch = self.visible_epoch;
        let mut reused = 0usize;

        for path in visible_paths {
            match self.slot_by_path.entry(path) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    reused += usize::from(entry.get().visible_epoch != visible_epoch);
                    entry.get_mut().visible_epoch = visible_epoch;
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(ShellVisibleItemSlot {
                        slot_id: 0,
                        visible_epoch,
                    });
                }
            }
        }

        let free_slots = &mut self.free_slots;
        self.slot_by_path.retain(|_, slot| {
            if slot.visible_epoch == visible_epoch {
                true
            } else {
                if slot.slot_id != 0 {
                    free_slots.push(slot.slot_id);
                }
                false
            }
        });
        if self.free_slots.len() > Self::MAX_FREE_SLOTS {
            self.free_slots.truncate(Self::MAX_FREE_SLOTS);
        }

        let mut recycled = 0usize;
        let mut allocated = 0usize;
        for slot in self.slot_by_path.values_mut() {
            if slot.slot_id != 0 {
                continue;
            }
            if let Some(slot_id) = self.free_slots.pop() {
                slot.slot_id = slot_id;
                recycled += 1;
            } else {
                self.next_slot_id += 1;
                slot.slot_id = self.next_slot_id;
                allocated += 1;
            }
        }

        ShellVisibleItemSlotStats {
            active: self.slot_by_path.len(),
            free: self.free_slots.len(),
            reused,
            recycled,
            allocated,
        }
    }

    pub(crate) fn slot_for_path(&self, path: &Path) -> Option<u64> {
        self.slot_by_path
            .get(path)
            .map(|slot| slot.slot_id)
            .filter(|slot_id| *slot_id != 0)
    }

    pub(crate) fn clear(&mut self) {
        self.slot_by_path.clear();
        self.free_slots.clear();
        self.visible_epoch = 0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneSplitMetrics {
    pub(crate) divider: ViewRect,
    pub(crate) right_pane: ViewRect,
    pub(crate) left_width: f32,
}
