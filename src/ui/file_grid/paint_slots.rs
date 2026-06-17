use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use fika_core::{ItemId, ItemLayout};
use gpui::SharedString;

use crate::ui::icons::FileIconSnapshot;

use super::details::{DetailsItemSnapshot, DetailsLayoutMetrics};
use super::snapshot::VisibleItemSnapshot;
use super::{FileGridRenderSnapshot, FileGridSnapshot};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ItemPaintSlotStats {
    pub(crate) inserted: usize,
    pub(crate) content_changed: usize,
    pub(crate) geometry_changed: usize,
    pub(crate) visual_changed: usize,
    pub(crate) unchanged: usize,
    pub(crate) removed: usize,
    pub(crate) entries: usize,
}

impl ItemPaintSlotStats {
    pub(crate) fn has_activity(self) -> bool {
        self.inserted > 0
            || self.content_changed > 0
            || self.geometry_changed > 0
            || self.visual_changed > 0
            || self.unchanged > 0
            || self.removed > 0
    }
}

#[derive(Default)]
pub(crate) struct ItemPaintSlotCache {
    slots: HashMap<u64, ItemPaintSlot>,
    details_slots: HashMap<ItemId, DetailsPaintSlot>,
    visible_epoch: u64,
}

pub(crate) struct ItemPaintSlotProjection {
    pub(crate) stats: ItemPaintSlotStats,
    pub(crate) snapshot: FileGridRenderSnapshot,
}

impl ItemPaintSlotCache {
    pub(crate) fn project_file_grid_snapshot(
        &mut self,
        snapshot: FileGridSnapshot,
        hovered_item: Option<ItemId>,
    ) -> ItemPaintSlotProjection {
        match snapshot {
            FileGridSnapshot::Compact { layout, items } => {
                let (mut stats, items) = self.project_visible_items(items, hovered_item);
                self.clear_details_items(&mut stats);
                self.finish_stats(&mut stats);
                ItemPaintSlotProjection {
                    stats,
                    snapshot: FileGridRenderSnapshot::Compact { layout, items },
                }
            }
            FileGridSnapshot::Icons { layout, items } => {
                let (mut stats, items) = self.project_visible_items(items, hovered_item);
                self.clear_details_items(&mut stats);
                self.finish_stats(&mut stats);
                ItemPaintSlotProjection {
                    stats,
                    snapshot: FileGridRenderSnapshot::Icons { layout, items },
                }
            }
            FileGridSnapshot::Details {
                items,
                row_count,
                metrics,
                name_column_width,
            } => {
                let (mut stats, _) = self.project_visible_items(Vec::new(), None);
                let items = self.project_details_items(
                    items,
                    metrics,
                    name_column_width,
                    hovered_item,
                    &mut stats,
                );
                self.finish_stats(&mut stats);
                ItemPaintSlotProjection {
                    stats,
                    snapshot: FileGridRenderSnapshot::Details {
                        items,
                        row_count,
                        metrics,
                        name_column_width,
                    },
                }
            }
        }
    }

    fn finish_stats(&self, stats: &mut ItemPaintSlotStats) {
        stats.entries = self.slots.len() + self.details_slots.len();
    }

    fn project_visible_items(
        &mut self,
        items: Vec<VisibleItemSnapshot>,
        hovered_item: Option<ItemId>,
    ) -> (ItemPaintSlotStats, Vec<ItemPaintSnapshot>) {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let mut stats = ItemPaintSlotStats::default();
        let mut snapshots = Vec::with_capacity(items.len());
        for item in items {
            let slot_id = item.slot_id;
            let item_id = item.item_id;
            let geometry = ItemPaintGeometry::from_layout(item.layout);
            let next_content = ItemPaintContent::from_item(&item);
            let visual = ItemPaintVisualState::from_item(&item, hovered_item);
            match self.slots.get_mut(&item.slot_id) {
                Some(slot) => {
                    if slot.content.as_ref() != &next_content {
                        stats.content_changed += 1;
                        slot.content = Arc::new(next_content);
                    } else if slot.geometry != geometry {
                        stats.geometry_changed += 1;
                    } else if slot.visual != visual {
                        stats.visual_changed += 1;
                    } else {
                        stats.unchanged += 1;
                    }
                    slot.item_id = item_id;
                    slot.visible = item.visible;
                    slot.geometry = geometry;
                    slot.visual = visual;
                    slot.visible_epoch = self.visible_epoch;
                    snapshots.push(slot.snapshot(slot_id, item.layout));
                }
                None => {
                    stats.inserted += 1;
                    let slot = ItemPaintSlot {
                        item_id,
                        visible: item.visible,
                        geometry,
                        content: Arc::new(next_content),
                        visual,
                        visible_epoch: self.visible_epoch,
                    };
                    snapshots.push(slot.snapshot(slot_id, item.layout));
                    self.slots.insert(slot_id, slot);
                }
            }
        }

        let visible_epoch = self.visible_epoch;
        let before_retain = self.slots.len();
        self.slots
            .retain(|_, slot| slot.visible_epoch == visible_epoch);
        stats.removed = before_retain.saturating_sub(self.slots.len());
        stats.entries = self.slots.len();
        (stats, snapshots)
    }

    fn clear_details_items(&mut self, stats: &mut ItemPaintSlotStats) {
        stats.removed += self.details_slots.len();
        self.details_slots.clear();
    }

    fn project_details_items(
        &mut self,
        items: Vec<DetailsItemSnapshot>,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
        hovered_item: Option<ItemId>,
        stats: &mut ItemPaintSlotStats,
    ) -> Vec<DetailsPaintSnapshot> {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let mut snapshots = Vec::with_capacity(items.len());
        for item in items {
            let item_id = item.item_id;
            let geometry = DetailsPaintGeometry::from_item(&item, metrics, name_column_width);
            let next_content = DetailsPaintContent::from_item(&item);
            let visual = DetailsPaintVisualState::from_item(&item, hovered_item);
            match self.details_slots.get_mut(&item_id) {
                Some(slot) => {
                    if slot.content.as_ref() != &next_content {
                        stats.content_changed += 1;
                        slot.content = Arc::new(next_content);
                    } else if slot.geometry != geometry {
                        stats.geometry_changed += 1;
                    } else if slot.visual != visual {
                        stats.visual_changed += 1;
                    } else {
                        stats.unchanged += 1;
                    }
                    slot.row_index = item.row_index;
                    slot.geometry = geometry;
                    slot.visual = visual;
                    slot.visible_epoch = self.visible_epoch;
                    snapshots.push(slot.snapshot(item_id));
                }
                None => {
                    stats.inserted += 1;
                    let slot = DetailsPaintSlot {
                        row_index: item.row_index,
                        geometry,
                        content: Arc::new(next_content),
                        visual,
                        visible_epoch: self.visible_epoch,
                    };
                    snapshots.push(slot.snapshot(item_id));
                    self.details_slots.insert(item_id, slot);
                }
            }
        }

        let visible_epoch = self.visible_epoch;
        let before_retain = self.details_slots.len();
        self.details_slots
            .retain(|_, slot| slot.visible_epoch == visible_epoch);
        stats.removed += before_retain.saturating_sub(self.details_slots.len());
        snapshots
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ItemPaintSlot {
    item_id: ItemId,
    visible: bool,
    geometry: ItemPaintGeometry,
    content: Arc<ItemPaintContent>,
    visual: ItemPaintVisualState,
    visible_epoch: u64,
}

impl ItemPaintSlot {
    fn snapshot(&self, slot_id: u64, layout: ItemLayout) -> ItemPaintSnapshot {
        ItemPaintSnapshot {
            slot_id,
            visible: self.visible,
            item_id: self.item_id,
            layout,
            content: self.content.clone(),
            visual: self.visual,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ItemPaintGeometry {
    item_x: u32,
    item_y: u32,
    item_width: u32,
    item_height: u32,
    visual_x: u32,
    visual_y: u32,
    visual_width: u32,
    visual_height: u32,
    icon_x: u32,
    icon_y: u32,
    icon_width: u32,
    icon_height: u32,
    text_x: u32,
    text_y: u32,
    text_width: u32,
    text_height: u32,
}

impl ItemPaintGeometry {
    fn from_layout(layout: ItemLayout) -> Self {
        let item = layout.item_rect;
        let visual = layout.visual_rect;
        let icon = layout.icon_rect;
        let text = layout.text_rect;
        Self {
            item_x: item.x.to_bits(),
            item_y: item.y.to_bits(),
            item_width: item.width.to_bits(),
            item_height: item.height.to_bits(),
            visual_x: visual.x.to_bits(),
            visual_y: visual.y.to_bits(),
            visual_width: visual.width.to_bits(),
            visual_height: visual.height.to_bits(),
            icon_x: icon.x.to_bits(),
            icon_y: icon.y.to_bits(),
            icon_width: icon.width.to_bits(),
            icon_height: icon.height.to_bits(),
            text_x: text.x.to_bits(),
            text_y: text.y.to_bits(),
            text_width: text.width.to_bits(),
            text_height: text.height.to_bits(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ItemPaintSnapshot {
    pub(super) slot_id: u64,
    pub(super) visible: bool,
    pub(super) item_id: ItemId,
    pub(super) layout: ItemLayout,
    pub(super) content: Arc<ItemPaintContent>,
    pub(super) visual: ItemPaintVisualState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ItemPaintContent {
    pub(super) item_id: ItemId,
    pub(super) is_dir: bool,
    pub(super) name: Arc<str>,
    pub(super) display_name: SharedString,
    pub(super) thumbnail_path: Option<Arc<Path>>,
    pub(super) icon: FileIconSnapshot,
    pub(super) fallback_marker: SharedString,
    pub(super) icon_name_lines: Arc<[SharedString]>,
    pub(super) drag_path: Arc<Path>,
    pub(super) draft_name: Option<String>,
    pub(super) draft_caret: Option<usize>,
    pub(super) draft_selection: Option<(usize, usize)>,
    pub(super) draft_error: Option<String>,
    pub(super) draft_warning: Option<String>,
}

impl ItemPaintContent {
    fn from_item(item: &VisibleItemSnapshot) -> Self {
        Self {
            item_id: item.item_id,
            is_dir: item.is_dir,
            name: item.name.clone(),
            display_name: item.display_name.clone(),
            thumbnail_path: item.thumbnail_path.clone(),
            icon: item.icon.clone(),
            fallback_marker: item.fallback_marker.clone(),
            icon_name_lines: item.icon_name_lines.clone(),
            drag_path: item.drag_path.clone(),
            draft_name: item.draft_name.clone(),
            draft_caret: item.draft_caret,
            draft_selection: item.draft_selection,
            draft_error: item.draft_error.clone(),
            draft_warning: item.draft_warning.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DetailsPaintSlot {
    row_index: usize,
    geometry: DetailsPaintGeometry,
    content: Arc<DetailsPaintContent>,
    visual: DetailsPaintVisualState,
    visible_epoch: u64,
}

impl DetailsPaintSlot {
    fn snapshot(&self, item_id: ItemId) -> DetailsPaintSnapshot {
        DetailsPaintSnapshot {
            item_id,
            row_index: self.row_index,
            geometry: self.geometry,
            content: self.content.clone(),
            visual: self.visual,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DetailsPaintSnapshot {
    pub(super) item_id: ItemId,
    pub(super) row_index: usize,
    pub(super) geometry: DetailsPaintGeometry,
    pub(super) content: Arc<DetailsPaintContent>,
    pub(super) visual: DetailsPaintVisualState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DetailsPaintGeometry {
    pub(super) row_top: u32,
    pub(super) row_height: u32,
    pub(super) icon_size: u32,
    pub(super) name_column_width: u32,
}

impl DetailsPaintGeometry {
    fn from_item(
        item: &DetailsItemSnapshot,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    ) -> Self {
        Self {
            row_top: (metrics.header_height + item.row_index as f32 * metrics.row_height).to_bits(),
            row_height: metrics.row_height.to_bits(),
            icon_size: metrics.icon_size.to_bits(),
            name_column_width: name_column_width.to_bits(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DetailsPaintContent {
    pub(super) path: Arc<Path>,
    pub(super) is_dir: bool,
    pub(super) name: Arc<str>,
    pub(super) icon: FileIconSnapshot,
    pub(super) size_label: String,
    pub(super) modified_label: String,
    pub(super) original_path_label: String,
    pub(super) deletion_time_label: String,
}

impl DetailsPaintContent {
    fn from_item(item: &DetailsItemSnapshot) -> Self {
        Self {
            path: Arc::from(item.path.as_path()),
            is_dir: item.is_dir,
            name: item.name.clone(),
            icon: item.icon.clone(),
            size_label: item.size_label.clone(),
            modified_label: item.modified_label.clone(),
            original_path_label: item.original_path_label.clone(),
            deletion_time_label: item.deletion_time_label.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DetailsPaintVisualState {
    pub(super) selected: bool,
    pub(super) selection_count: usize,
    pub(super) hovered: bool,
    pub(super) drop_target: bool,
}

impl DetailsPaintVisualState {
    fn from_item(item: &DetailsItemSnapshot, hovered_item: Option<ItemId>) -> Self {
        Self {
            selected: item.selected,
            selection_count: item.selection_count,
            hovered: hovered_item == Some(item.item_id),
            drop_target: item.drop_target,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ItemPaintVisualState {
    pub(super) selected: bool,
    pub(super) selection_count: usize,
    pub(super) hovered: bool,
    pub(super) drop_target: bool,
}

impl ItemPaintVisualState {
    fn from_item(item: &VisibleItemSnapshot, hovered_item: Option<ItemId>) -> Self {
        Self {
            selected: item.selected,
            selection_count: item.selection_count,
            hovered: hovered_item == Some(item.item_id),
            drop_target: item.drop_target,
        }
    }
}
