use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::ui::icons::FileIconSnapshot;
use crate::ui::retained::RetainedSlotStats;

use super::PlaceSnapshot;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PlacePaintSlotStats {
    pub(crate) rows: usize,
    pub(crate) sections: usize,
    retained: RetainedSlotStats,
}

impl Deref for PlacePaintSlotStats {
    type Target = RetainedSlotStats;

    fn deref(&self) -> &Self::Target {
        &self.retained
    }
}

impl DerefMut for PlacePaintSlotStats {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.retained
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacePaintSlotPerfLog {
    pub(crate) stats: PlacePaintSlotStats,
    pub(crate) elapsed: Duration,
}

#[derive(Default)]
pub(crate) struct PlacePaintSlotCache {
    slots: HashMap<PlacePaintSlotKey, PlacePaintSlot>,
    visible_epoch: u64,
}

impl PlacePaintSlotCache {
    pub(crate) fn project_snapshots(&mut self, places: &[PlaceSnapshot]) -> PlacePaintSlotStats {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let mut stats = PlacePaintSlotStats::default();
        let mut current_group = None;
        let mut ordinal = 0;

        for place in places {
            if current_group != Some(place.group) {
                current_group = Some(place.group);
                if !place.group.is_empty() {
                    self.project_section_slot(place.group, ordinal, &mut stats);
                    ordinal += 1;
                }
            }
            self.project_place_slot(place, ordinal, &mut stats);
            ordinal += 1;
        }

        let visible_epoch = self.visible_epoch;
        let before_retain = self.slots.len();
        self.slots
            .retain(|_, slot| slot.visible_epoch == visible_epoch);
        stats.removed = before_retain.saturating_sub(self.slots.len());
        stats.entries = self.slots.len();
        stats
    }

    fn project_section_slot(
        &mut self,
        group: &'static str,
        ordinal: usize,
        stats: &mut PlacePaintSlotStats,
    ) {
        stats.sections += 1;
        let key = PlacePaintSlotKey::Section(group);
        let geometry = PlacePaintGeometry { ordinal };
        let content = PlacePaintContent::Section { group };
        let visual = PlacePaintVisualState::default();
        self.project_slot(key, geometry, content, visual, stats);
    }

    fn project_place_slot(
        &mut self,
        place: &PlaceSnapshot,
        ordinal: usize,
        stats: &mut PlacePaintSlotStats,
    ) {
        stats.rows += 1;
        let key = PlacePaintSlotKey::from_place(place);
        let geometry = PlacePaintGeometry { ordinal };
        let content = PlacePaintContent::Place {
            group: place.group,
            label: place.label.clone(),
            path: place.path.clone(),
            device_id: place.device_id.clone(),
            icon: place.icon.clone(),
            device: place.device,
            network: place.network,
            device_ejectable: place.device_ejectable,
            device_can_power_off: place.device_can_power_off,
            trash_place: place.trash_place,
            editable: place.editable,
            removable: place.removable,
        };
        let visual = PlacePaintVisualState {
            active: place.active,
            mounted: place.mounted,
            drop_target: place.drop_target,
            insert_before: place.insert_before,
            insert_after: place.insert_after,
            trash_has_items: place.trash_has_items,
        };
        self.project_slot(key, geometry, content, visual, stats);
    }

    fn project_slot(
        &mut self,
        key: PlacePaintSlotKey,
        geometry: PlacePaintGeometry,
        content: PlacePaintContent,
        visual: PlacePaintVisualState,
        stats: &mut PlacePaintSlotStats,
    ) {
        match self.slots.get_mut(&key) {
            Some(slot) => {
                if slot.content.as_ref() != &content {
                    stats.content_changed += 1;
                    slot.content = Arc::new(content);
                } else if slot.geometry != geometry {
                    stats.geometry_changed += 1;
                } else if slot.visual != visual {
                    stats.visual_changed += 1;
                } else {
                    stats.unchanged += 1;
                }
                slot.geometry = geometry;
                slot.visual = visual;
                slot.visible_epoch = self.visible_epoch;
            }
            None => {
                stats.inserted += 1;
                self.slots.insert(
                    key,
                    PlacePaintSlot {
                        geometry,
                        content: Arc::new(content),
                        visual,
                        visible_epoch: self.visible_epoch,
                    },
                );
            }
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum PlacePaintSlotKey {
    Section(&'static str),
    Place(PlacePaintIdentity),
}

impl PlacePaintSlotKey {
    fn from_place(place: &PlaceSnapshot) -> Self {
        let identity = match &place.device_id {
            Some(device_id) if place.device => PlacePaintIdentity::Device {
                device_id: device_id.clone(),
            },
            _ => PlacePaintIdentity::Path {
                group: place.group,
                path: place.path.clone(),
            },
        };
        Self::Place(identity)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum PlacePaintIdentity {
    Device { device_id: String },
    Path { group: &'static str, path: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlacePaintSlot {
    geometry: PlacePaintGeometry,
    content: Arc<PlacePaintContent>,
    visual: PlacePaintVisualState,
    visible_epoch: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlacePaintGeometry {
    ordinal: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PlacePaintContent {
    Section {
        group: &'static str,
    },
    Place {
        group: &'static str,
        label: String,
        path: PathBuf,
        device_id: Option<String>,
        icon: FileIconSnapshot,
        device: bool,
        network: bool,
        device_ejectable: bool,
        device_can_power_off: bool,
        trash_place: bool,
        editable: bool,
        removable: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PlacePaintVisualState {
    active: bool,
    mounted: bool,
    drop_target: bool,
    insert_before: bool,
    insert_after: bool,
    trash_has_items: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_paint_slots_insert_then_remain_unchanged() {
        let mut cache = PlacePaintSlotCache::default();
        let places = vec![place("", "Home", "/home/yk"), place("Devices", "Root", "/")];

        let first = cache.project_snapshots(&places);
        assert_eq!(first.rows, 2);
        assert_eq!(first.sections, 1);
        assert_eq!(first.inserted, 3);
        assert_eq!(first.entries, 3);

        let second = cache.project_snapshots(&places);
        assert_eq!(second.unchanged, 3);
        assert_eq!(second.entries, 3);
    }

    #[test]
    fn place_paint_slot_stats_wrap_retained_slot_delta_stats() {
        let mut stats = PlacePaintSlotStats::default();

        stats.inserted += 1;
        stats.content_changed += 2;
        stats.entries = 3;

        assert_eq!(stats.rows, 0);
        assert_eq!(stats.sections, 0);
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.content_changed, 2);
        assert_eq!(stats.entries, 3);
    }

    #[test]
    fn place_paint_slots_classify_content_visual_geometry_and_removed() {
        let mut cache = PlacePaintSlotCache::default();
        let mut places = vec![place("", "Home", "/home/yk"), place("", "Docs", "/docs")];
        cache.project_snapshots(&places);

        places[0].active = true;
        let visual = cache.project_snapshots(&places);
        assert_eq!(visual.visual_changed, 1);
        assert_eq!(visual.unchanged, 1);

        places[0].label = "Start".to_string();
        let content = cache.project_snapshots(&places);
        assert_eq!(content.content_changed, 1);
        assert_eq!(content.unchanged, 1);

        places.swap(0, 1);
        let geometry = cache.project_snapshots(&places);
        assert_eq!(geometry.geometry_changed, 2);

        let removed = cache.project_snapshots(&places[..1]);
        assert_eq!(removed.removed, 1);
        assert_eq!(removed.entries, 1);
    }

    #[test]
    fn place_paint_slots_prefer_device_identity() {
        let mut cache = PlacePaintSlotCache::default();
        let mut device = place("Devices", "USB", "/run/media/USB-A");
        device.device = true;
        device.device_id = Some("gio:test-usb".to_string());
        cache.project_snapshots(&[device.clone()]);

        device.path = PathBuf::from("/run/media/USB-B");
        let stats = cache.project_snapshots(&[device]);

        assert_eq!(stats.content_changed, 1);
        assert_eq!(stats.inserted, 0);
        assert_eq!(stats.removed, 0);
    }

    fn place(group: &'static str, label: &str, path: &str) -> PlaceSnapshot {
        PlaceSnapshot {
            index: 0,
            group,
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
            label: label.to_string(),
            path: PathBuf::from(path),
            device_id: None,
            mounted: true,
            device: false,
            network: false,
            device_ejectable: false,
            device_can_power_off: false,
            active: false,
            drop_target: false,
            insert_before: false,
            insert_after: false,
            trash_place: false,
            trash_has_items: false,
            editable: false,
            removable: false,
        }
    }
}
