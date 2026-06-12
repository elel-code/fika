use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub(crate) struct VisibleItemSlotPool {
    next_slot_id: u64,
    pub(crate) slot_by_item_id: BTreeMap<fika_core::ItemId, u64>,
    pub(crate) free_slots: Vec<u64>,
}

impl VisibleItemSlotPool {
    pub(crate) const MAX_FREE_SLOTS: usize = 100;

    pub(crate) fn slots_for_items(
        &mut self,
        visible_item_ids: impl IntoIterator<Item = fika_core::ItemId>,
    ) -> BTreeMap<fika_core::ItemId, u64> {
        let visible_item_ids = visible_item_ids.into_iter().collect::<BTreeSet<_>>();
        let previous = std::mem::take(&mut self.slot_by_item_id);
        for (item_id, slot_id) in previous {
            if visible_item_ids.contains(&item_id) {
                self.slot_by_item_id.insert(item_id, slot_id);
            } else {
                self.free_slots.push(slot_id);
            }
        }
        if self.free_slots.len() > Self::MAX_FREE_SLOTS {
            self.free_slots.truncate(Self::MAX_FREE_SLOTS);
        }

        for item_id in visible_item_ids {
            if self.slot_by_item_id.contains_key(&item_id) {
                continue;
            }
            let slot_id = self.free_slots.pop().unwrap_or_else(|| {
                self.next_slot_id += 1;
                self.next_slot_id
            });
            self.slot_by_item_id.insert(item_id, slot_id);
        }

        self.slot_by_item_id.clone()
    }
}
