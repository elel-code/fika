use std::collections::HashMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VisibleItemSlot {
    slot_id: u64,
    visible_epoch: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VisibleItemSlotPool {
    next_slot_id: u64,
    visible_epoch: u64,
    pub(crate) slot_by_item_id: HashMap<fika_core::ItemId, VisibleItemSlot>,
    pub(crate) free_slots: Vec<u64>,
}

impl VisibleItemSlotPool {
    pub(crate) const MAX_FREE_SLOTS: usize = 100;

    pub(crate) fn update_visible_items(
        &mut self,
        visible_item_ids: impl IntoIterator<Item = fika_core::ItemId>,
    ) {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let visible_epoch = self.visible_epoch;

        for item_id in visible_item_ids {
            match self.slot_by_item_id.entry(item_id) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().visible_epoch = visible_epoch;
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(VisibleItemSlot {
                        slot_id: 0,
                        visible_epoch,
                    });
                }
            }
        }

        let free_slots = &mut self.free_slots;
        self.slot_by_item_id.retain(|_, slot| {
            if slot.visible_epoch == visible_epoch {
                true
            } else {
                free_slots.push(slot.slot_id);
                false
            }
        });
        if self.free_slots.len() > Self::MAX_FREE_SLOTS {
            self.free_slots.truncate(Self::MAX_FREE_SLOTS);
        }

        for slot in self.slot_by_item_id.values_mut() {
            if slot.slot_id != 0 {
                continue;
            }
            slot.slot_id = self.free_slots.pop().unwrap_or_else(|| {
                self.next_slot_id += 1;
                self.next_slot_id
            });
        }
    }

    pub(crate) fn slot_for_item(&self, item_id: fika_core::ItemId) -> Option<u64> {
        self.slot_by_item_id
            .get(&item_id)
            .map(|slot| slot.slot_id)
            .filter(|slot_id| *slot_id != 0)
    }
}
