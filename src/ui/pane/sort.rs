use fika_core::{SortOrder, SortRole};

pub(crate) fn sort_role_label(role: SortRole) -> &'static str {
    match role {
        SortRole::Name => "Name",
        SortRole::Modified => "Modified",
        SortRole::Size => "Size",
        SortRole::TrashOriginalPath => "Original Path",
        SortRole::TrashDeletionTime => "Deletion Time",
    }
}

pub(crate) fn sort_order_label(order: SortOrder) -> &'static str {
    match order {
        SortOrder::Ascending => "Ascending",
        SortOrder::Descending => "Descending",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_labels_match_pane_status_text() {
        assert_eq!(sort_role_label(SortRole::Name), "Name");
        assert_eq!(sort_role_label(SortRole::Modified), "Modified");
        assert_eq!(sort_role_label(SortRole::Size), "Size");
        assert_eq!(
            sort_role_label(SortRole::TrashOriginalPath),
            "Original Path"
        );
        assert_eq!(
            sort_role_label(SortRole::TrashDeletionTime),
            "Deletion Time"
        );
        assert_eq!(sort_order_label(SortOrder::Ascending), "Ascending");
        assert_eq!(sort_order_label(SortOrder::Descending), "Descending");
    }
}
