use crate::app::pane::{PaneEntryModel, PaneTarget, PreparedDirectoryEntries};
use crate::app::state::AppState;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

#[derive(Debug)]
pub(crate) struct DirectoryLoadPreparation {
    pub(crate) pane_id: u64,
    pub(crate) current_dir: PathBuf,
    pub(crate) generation: u64,
    pub(crate) cached_entries: Option<PreparedDirectoryEntries>,
    pub(crate) defer_view_restore: bool,
}

pub(crate) fn prepare_directory_load(
    state: &mut AppState,
    preserve_view: bool,
) -> DirectoryLoadPreparation {
    prepare_directory_load_for_target(state, PaneTarget::Focused, preserve_view)
        .expect("focused pane should always exist")
}

pub(crate) fn prepare_directory_load_for_target(
    state: &mut AppState,
    target: PaneTarget,
    preserve_view: bool,
) -> Option<DirectoryLoadPreparation> {
    let (pane_id, current_dir, generation) = {
        let pane = state.panes.pane_mut_for_target(target)?;
        if let Some(cancel) = pane.search_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let generation = pane.load_generation.next();
        pane.open_generation.next();
        pane.search_generation.next();
        if !preserve_view {
            pane.thumbnail_generation.next();
            pane.view.clear_thumbnail_pending();
            pane.view.invalidate_virtual_view();
            pane.selection.clear();
            pane.search.reset_all();
        }
        (pane.id, pane.current_dir.clone(), generation)
    };
    let cached_entries = state.cached_directory_entries(&current_dir);
    let defer_view_restore = !preserve_view && cached_entries.is_none();

    Some(DirectoryLoadPreparation {
        pane_id,
        current_dir,
        generation,
        cached_entries,
        defer_view_restore,
    })
}

pub(crate) fn directory_entries_match(
    current_entries: &PaneEntryModel,
    incoming_entries: &PreparedDirectoryEntries,
) -> bool {
    current_entries == &incoming_entries.entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::entries::RawFileEntry;
    use crate::fs::thumbnails;
    use std::path::Path;

    #[test]
    fn preserved_directory_reload_keeps_thumbnail_pipeline_and_view_context() {
        let mut state = AppState::new(PathBuf::from("/tmp/current"), Vec::new());
        let pending_key = thumbnails::fallback_key(Path::new("/tmp/current/photo.png"), 64);
        state
            .panes
            .focused_mut()
            .view
            .insert_thumbnail_pending("/tmp/current/photo.png".to_string(), pending_key.clone());
        state.panes.focused_mut().search.query = "photo".to_string();
        state.panes.focused_mut().selection.paths = vec!["/tmp/current/photo.png".to_string()];
        state.panes.focused_mut().selection.anchor = Some("/tmp/current/photo.png".to_string());
        state.insert_directory_cache(
            PathBuf::from("/tmp/current"),
            test_entries(vec![("photo.png", "/tmp/current/photo.png")]),
        );
        let thumbnail_generation = state.panes.focused().thumbnail_generation.current();

        let preparation = prepare_directory_load(&mut state, true);

        assert_eq!(preparation.current_dir, PathBuf::from("/tmp/current"));
        assert_eq!(
            preparation
                .cached_entries
                .as_ref()
                .map(|entries| entries.len()),
            Some(1)
        );
        assert!(!preparation.defer_view_restore);
        assert_eq!(
            state.panes.focused().thumbnail_generation.current(),
            thumbnail_generation
        );
        assert_eq!(
            state
                .panes
                .focused()
                .view
                .thumbnail_pending_key("/tmp/current/photo.png"),
            Some(&pending_key)
        );
        assert_eq!(state.panes.focused().search.query, "photo");
        assert_eq!(
            state.panes.focused().selection.paths,
            vec!["/tmp/current/photo.png"]
        );
        assert_eq!(
            state.panes.focused().selection.anchor.as_deref(),
            Some("/tmp/current/photo.png")
        );
    }

    #[test]
    fn new_directory_load_cancels_thumbnail_pipeline_and_view_context() {
        let mut state = AppState::new(PathBuf::from("/tmp/current"), Vec::new());
        let pending_key = thumbnails::fallback_key(Path::new("/tmp/current/photo.png"), 64);
        state
            .panes
            .focused_mut()
            .view
            .insert_thumbnail_pending("/tmp/current/photo.png".to_string(), pending_key);
        state.panes.focused_mut().search.query = "photo".to_string();
        state.panes.focused_mut().search.kind_filter = 3;
        state.panes.focused_mut().selection.paths = vec!["/tmp/current/photo.png".to_string()];
        state.panes.focused_mut().selection.anchor = Some("/tmp/current/photo.png".to_string());
        let thumbnail_generation = state.panes.focused().thumbnail_generation.current();

        let preparation = prepare_directory_load(&mut state, false);

        assert_eq!(preparation.current_dir, PathBuf::from("/tmp/current"));
        assert!(preparation.cached_entries.is_none());
        assert!(preparation.defer_view_restore);
        assert!(state.panes.focused().thumbnail_generation.current() > thumbnail_generation);
        assert!(
            !state
                .panes
                .focused()
                .view
                .has_thumbnail_pending("/tmp/current/photo.png")
        );
        assert!(state.panes.focused().search.query.is_empty());
        assert_eq!(state.panes.focused().search.kind_filter, 0);
        assert!(state.panes.focused().selection.paths.is_empty());
        assert!(state.panes.focused().selection.anchor.is_none());
    }

    #[test]
    fn targeted_directory_load_updates_only_requested_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/active"), Vec::new());
        state.panes.focused_mut().search.query = "active-query".to_string();
        state.panes.focused_mut().selection.paths = vec!["/tmp/active/keep.txt".to_string()];
        let active_pending_key = thumbnails::fallback_key(Path::new("/tmp/active/keep.txt"), 64);
        state
            .panes
            .focused_mut()
            .view
            .insert_thumbnail_pending("/tmp/active/keep.txt".to_string(), active_pending_key);
        assert!(state.panes.open_pane(PathBuf::from("/tmp/inactive")));
        let inactive_id = state.panes.pane_for_slot(1).expect("inactive pane").id;
        {
            let inactive = state.panes.pane_mut_for_slot(1).expect("inactive pane");
            inactive.search.query = "inactive-query".to_string();
            inactive.selection.paths = vec!["/tmp/inactive/drop.txt".to_string()];
            inactive.selection.anchor = Some("/tmp/inactive/drop.txt".to_string());
            inactive.view.insert_thumbnail_pending(
                "/tmp/inactive/drop.txt".to_string(),
                thumbnails::fallback_key(Path::new("/tmp/inactive/drop.txt"), 64),
            );
        }

        let preparation =
            prepare_directory_load_for_target(&mut state, PaneTarget::Id(inactive_id), false)
                .expect("inactive pane should resolve");

        assert_eq!(preparation.pane_id, inactive_id);
        assert_eq!(preparation.current_dir, PathBuf::from("/tmp/inactive"));
        assert_eq!(state.panes.focused().search.query, "active-query");
        assert_eq!(
            state.panes.focused().selection.paths,
            vec!["/tmp/active/keep.txt"]
        );
        assert!(
            state
                .panes
                .focused()
                .view
                .has_thumbnail_pending("/tmp/active/keep.txt")
        );

        let inactive = state.panes.pane_for_slot(1).expect("inactive pane");
        assert!(inactive.search.query.is_empty());
        assert!(inactive.selection.paths.is_empty());
        assert!(inactive.selection.anchor.is_none());
        assert!(
            !inactive
                .view
                .has_thumbnail_pending("/tmp/inactive/drop.txt")
        );
        assert!(inactive.view.virtual_view.range.is_empty());
    }

    #[test]
    fn cached_directory_load_restores_view_before_async_refresh() {
        let mut state = AppState::new(PathBuf::from("/tmp/current"), Vec::new());
        state.insert_directory_cache(
            PathBuf::from("/tmp/current"),
            test_entries(vec![("cached.txt", "/tmp/current/cached.txt")]),
        );

        let preparation = prepare_directory_load(&mut state, false);

        assert!(preparation.cached_entries.is_some());
        assert!(!preparation.defer_view_restore);
    }

    fn test_entries(entries: Vec<(&str, &str)>) -> PreparedDirectoryEntries {
        PreparedDirectoryEntries::new(
            entries
                .into_iter()
                .map(|(name, path)| RawFileEntry {
                    name_width_units: crate::app::geometry::compact_text_width_units(name),
                    name: name.to_string(),
                    path: path.to_string(),
                    group: String::new(),
                    location: String::new(),
                    kind: "File".to_string(),
                    size: "1 KB".to_string(),
                    size_bytes: 1024,
                    modified: "Today".to_string(),
                    modified_age_days: 0,
                    is_dir: false,
                })
                .collect(),
        )
    }

    #[test]
    fn directory_entries_match_detects_equivalent_visible_model() {
        let current = test_entries(vec![("photo.png", "/tmp/current/photo.png")]);
        let incoming = test_entries(vec![("photo.png", "/tmp/current/photo.png")]);

        assert!(directory_entries_match(&current.entries, &incoming));
    }

    #[test]
    fn directory_entries_match_detects_visible_changes() {
        let current = test_entries(vec![("photo.png", "/tmp/current/photo.png")]);
        let incoming = test_entries(vec![("notes.txt", "/tmp/current/notes.txt")]);

        assert!(!directory_entries_match(&current.entries, &incoming));
    }
}
