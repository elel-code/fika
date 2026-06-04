use crate::app::state::AppState;
use std::sync::atomic::Ordering;

pub(crate) fn cancel_active_search(state: &mut AppState) {
    if let Some(cancel) = state.panes.focused_mut().search_cancel.take() {
        cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) fn reset_search_state(state: &mut AppState) {
    let pane = state.panes.focused_mut();
    pane.search.reset_all();
    pane.view.virtual_view.invalidate();
}

pub(crate) fn set_search_filters(state: &mut AppState, kind: i32, modified: i32, size: i32) {
    let pane = state.panes.focused_mut();
    pane.search.kind_filter = kind.clamp(0, 3);
    pane.search.modified_filter = modified.clamp(0, 3);
    pane.search.size_filter = size.clamp(0, 3);
}

pub(crate) fn search_filters_active(state: &AppState) -> bool {
    state.panes.focused().search.kind_filter != 0
        || state.panes.focused().search.modified_filter != 0
        || state.panes.focused().search.size_filter != 0
}

pub(crate) fn recursive_search_status(query: &str) -> String {
    format!("Searching recursively for '{query}'...")
}

pub(crate) fn recursive_search_progress_status(
    query: &str,
    directories_scanned: usize,
    matches: usize,
) -> String {
    if directories_scanned == 0 {
        return recursive_search_status(query);
    }

    format!(
        "Searching recursively for '{query}'... {matches} result(s), {directories_scanned} folder(s) scanned"
    )
}

pub(crate) fn recursive_search_finished_status(visible: usize, total: usize) -> String {
    if visible == total {
        format!("{total} recursive search result(s)")
    } else {
        format!("{visible} of {total} recursive search result(s) after filters")
    }
}

pub(crate) fn recursive_search_cancelled_status(
    query: &str,
    directories_scanned: usize,
    matches: usize,
) -> String {
    if directories_scanned == 0 {
        return format!("Recursive search for '{query}' cancelled");
    }

    format!(
        "Recursive search for '{query}' cancelled after {directories_scanned} folder(s); {matches} result(s) discarded"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn search_filters_are_clamped_and_reset() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());

        set_search_filters(&mut state, -1, 2, 99);

        assert_eq!(state.panes.focused().search.kind_filter, 0);
        assert_eq!(state.panes.focused().search.modified_filter, 2);
        assert_eq!(state.panes.focused().search.size_filter, 3);
        assert!(search_filters_active(&state));

        set_search_filters(&mut state, 0, 0, 0);

        assert_eq!(state.panes.focused().search.kind_filter, 0);
        assert_eq!(state.panes.focused().search.modified_filter, 0);
        assert_eq!(state.panes.focused().search.size_filter, 0);
        assert!(!search_filters_active(&state));
    }

    #[test]
    fn reset_search_state_clears_query_and_filters() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().search.query = "report".to_string();
        set_search_filters(&mut state, 1, 2, 3);
        state.panes.focused_mut().search.visible_entry_indices = Some(vec![0, 2, 4]);
        state.panes.focused_mut().view.virtual_view.range = 1..3;

        reset_search_state(&mut state);

        assert_eq!(state.panes.focused().search.query, "");
        assert_eq!(state.panes.focused().search.kind_filter, 0);
        assert_eq!(state.panes.focused().search.modified_filter, 0);
        assert_eq!(state.panes.focused().search.size_filter, 0);
        assert!(state.panes.focused().search.visible_entry_indices.is_none());
        assert!(state.panes.focused().view.virtual_view.range.is_empty());
    }

    #[test]
    fn cancel_active_search_marks_token_and_removes_it_from_state() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let cancel = Arc::new(AtomicBool::new(false));
        state.panes.focused_mut().search_cancel = Some(cancel.clone());

        cancel_active_search(&mut state);

        assert!(cancel.load(Ordering::Relaxed));
        assert!(state.panes.focused().search_cancel.is_none());
    }

    #[test]
    fn recursive_search_status_keeps_query_visible_during_background_scan() {
        assert_eq!(
            recursive_search_status("report"),
            "Searching recursively for 'report'..."
        );
        assert_eq!(
            recursive_search_progress_status("report", 12, 4),
            "Searching recursively for 'report'... 4 result(s), 12 folder(s) scanned"
        );
        assert_eq!(
            recursive_search_finished_status(4, 4),
            "4 recursive search result(s)"
        );
        assert_eq!(
            recursive_search_finished_status(2, 4),
            "2 of 4 recursive search result(s) after filters"
        );
        assert_eq!(
            recursive_search_cancelled_status("report", 12, 4),
            "Recursive search for 'report' cancelled after 12 folder(s); 4 result(s) discarded"
        );
        assert_eq!(
            recursive_search_cancelled_status("report", 3, 0),
            "Recursive search for 'report' cancelled after 3 folder(s); 0 result(s) discarded"
        );
        assert_eq!(
            recursive_search_cancelled_status("report", 0, 0),
            "Recursive search for 'report' cancelled"
        );
    }
}
