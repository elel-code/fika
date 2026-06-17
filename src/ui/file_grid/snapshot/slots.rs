use super::super::VisibleItemSlotPool;
use super::RawFileGridSnapshot;

impl RawFileGridSnapshot {
    pub(crate) fn assign_visible_item_slots(&mut self, slots: &mut VisibleItemSlotPool) {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                slots.update_visible_items(items.iter().map(|item| item.item_id));
                for item in items {
                    item.slot_id = slots.slot_for_item(item.item_id).unwrap_or_default();
                }
            }
            Self::Details { .. } => {}
        }
    }
}
