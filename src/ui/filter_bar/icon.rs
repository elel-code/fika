use crate::ui::icons::{FileIconCache, FileIconSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilterToggleSnapshot {
    pub(crate) active: bool,
    pub(crate) label: &'static str,
    pub(crate) icon: FileIconSnapshot,
}

pub(crate) fn filter_toggle_snapshot(
    cache: &mut FileIconCache,
    active: bool,
) -> FilterToggleSnapshot {
    let (name, candidates, label, marker, fallback_fg, fallback_bg) = if active {
        (
            "filter-close",
            &["window-close", "dialog-close", "edit-clear"][..],
            "Close",
            "Close",
            0x475569,
            0xf1f5f9,
        )
    } else {
        (
            "filter-search",
            &["edit-find", "system-search", "search"][..],
            "Search",
            "Search",
            0x1f4fbf,
            0xeaf1ff,
        )
    };
    FilterToggleSnapshot {
        active,
        label,
        icon: cache.named_icon(name, candidates, marker, fallback_fg, fallback_bg, 18.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_toggle_snapshot_uses_search_and_close_semantics() {
        let mut cache = FileIconCache::default();

        let search = filter_toggle_snapshot(&mut cache, false);
        assert!(!search.active);
        assert_eq!(search.label, "Search");
        assert!(matches!(
            search.icon.icon_name.as_ref(),
            "edit-find" | "system-search" | "search"
        ));

        let close = filter_toggle_snapshot(&mut cache, true);
        assert!(close.active);
        assert_eq!(close.label, "Close");
        assert!(matches!(
            close.icon.icon_name.as_ref(),
            "window-close" | "dialog-close" | "edit-clear"
        ));
    }
}
