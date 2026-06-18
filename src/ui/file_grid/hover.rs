use fika_core::{ItemId, PaneId};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RetainedHoveredItem {
    current: Option<(PaneId, ItemId)>,
}

impl RetainedHoveredItem {
    pub(crate) fn set(&mut self, pane_id: PaneId, item_id: ItemId) -> bool {
        let next = Some((pane_id, item_id));
        if self.current == next {
            return false;
        }
        self.current = next;
        true
    }

    pub(crate) fn clear_item(&mut self, pane_id: PaneId, item_id: ItemId) -> bool {
        if self.current != Some((pane_id, item_id)) {
            return false;
        }
        self.current = None;
        true
    }

    pub(crate) fn clear_pane(&mut self, pane_id: PaneId) -> bool {
        if !matches!(self.current, Some((hovered_pane, _)) if hovered_pane == pane_id) {
            return false;
        }
        self.current = None;
        true
    }

    pub(crate) fn item_for_pane(&self, pane_id: PaneId) -> Option<ItemId> {
        self.current
            .and_then(|(hovered_pane, item_id)| (hovered_pane == pane_id).then_some(item_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_hovered_item_reports_changes_only_when_state_changes() {
        let mut hovered = RetainedHoveredItem::default();

        assert_eq!(hovered.item_for_pane(PaneId(1)), None);
        assert!(hovered.set(PaneId(1), ItemId(7)));
        assert!(!hovered.set(PaneId(1), ItemId(7)));
        assert_eq!(hovered.item_for_pane(PaneId(1)), Some(ItemId(7)));
        assert_eq!(hovered.item_for_pane(PaneId(2)), None);

        assert!(!hovered.clear_item(PaneId(2), ItemId(7)));
        assert!(!hovered.clear_item(PaneId(1), ItemId(8)));
        assert!(hovered.clear_item(PaneId(1), ItemId(7)));
        assert!(!hovered.clear_item(PaneId(1), ItemId(7)));
        assert_eq!(hovered.item_for_pane(PaneId(1)), None);
    }

    #[test]
    fn retained_hovered_item_can_clear_by_pane() {
        let mut hovered = RetainedHoveredItem::default();

        assert!(hovered.set(PaneId(2), ItemId(9)));
        assert!(!hovered.clear_pane(PaneId(1)));
        assert_eq!(hovered.item_for_pane(PaneId(2)), Some(ItemId(9)));
        assert!(hovered.clear_pane(PaneId(2)));
        assert_eq!(hovered.item_for_pane(PaneId(2)), None);
    }
}
