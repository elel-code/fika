use crate::FileEntry;
use crate::app::pane::PaneTarget;
use crate::app::state::AppState;
use crate::fs::entries::RawFileEntry;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

#[derive(Debug)]
pub(crate) struct DirectoryLoadPreparation {
    pub(crate) pane_id: u64,
    pub(crate) current_dir: PathBuf,
    pub(crate) generation: u64,
    pub(crate) cached_entries: Option<Vec<FileEntry>>,
    pub(crate) defer_view_restore: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryLoadErrorRecovery {
    KeepVisibleModel,
    RollBackToItemsPath(PathBuf),
    ClearTarget,
}

pub(crate) fn prepare_directory_load(
    state: &mut AppState,
    preserve_view: bool,
) -> DirectoryLoadPreparation {
    prepare_directory_load_for_target(state, PaneTarget::Active, preserve_view)
        .expect("active pane should always exist")
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
            pane.view.virtual_view.invalidate();
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

pub(crate) fn directory_load_error_recovery(
    preserve_view: bool,
    target_path: &Path,
    items_path: &str,
    has_visible_entries: bool,
) -> DirectoryLoadErrorRecovery {
    let items_path = (!items_path.is_empty()).then(|| PathBuf::from(items_path));
    if preserve_view {
        return if items_path.is_some() || has_visible_entries {
            DirectoryLoadErrorRecovery::KeepVisibleModel
        } else {
            DirectoryLoadErrorRecovery::ClearTarget
        };
    }

    match items_path {
        Some(items_path) if items_path.as_path() == target_path => {
            DirectoryLoadErrorRecovery::KeepVisibleModel
        }
        Some(items_path) => DirectoryLoadErrorRecovery::RollBackToItemsPath(items_path),
        None if has_visible_entries => DirectoryLoadErrorRecovery::KeepVisibleModel,
        None => DirectoryLoadErrorRecovery::ClearTarget,
    }
}

pub(crate) fn directory_entries_match(
    current_entries: &[FileEntry],
    incoming_entries: &[RawFileEntry],
) -> bool {
    current_entries.len() == incoming_entries.len()
        && current_entries
            .iter()
            .zip(incoming_entries)
            .all(|(current, incoming)| file_entry_matches_raw(current, incoming))
}

fn file_entry_matches_raw(current: &FileEntry, incoming: &RawFileEntry) -> bool {
    current.name.as_str() == incoming.name
        && current.path.as_str() == incoming.path
        && current.group.as_str() == incoming.group
        && current.location.as_str() == incoming.location
        && current.kind.as_str() == incoming.kind
        && current.size.as_str() == incoming.size
        && current.size_bytes == incoming.size_bytes as f32
        && current.modified.as_str() == incoming.modified
        && current.modified_age_days == incoming.modified_age_days
        && current.is_dir == incoming.is_dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileEntry;
    use crate::fs::thumbnails;
    use slint::Image;
    use std::path::Path;

    #[test]
    fn preserved_directory_reload_keeps_thumbnail_pipeline_and_view_context() {
        let mut state = AppState::new(PathBuf::from("/tmp/current"), Vec::new());
        let pending_key = thumbnails::fallback_key(Path::new("/tmp/current/photo.png"), 64);
        state
            .panes
            .active_mut()
            .view
            .insert_thumbnail_pending("/tmp/current/photo.png".to_string(), pending_key.clone());
        state.panes.active_mut().search.query = "photo".to_string();
        state.panes.active_mut().selection.paths = vec!["/tmp/current/photo.png".to_string()];
        state.panes.active_mut().selection.anchor = Some("/tmp/current/photo.png".to_string());
        state.insert_directory_cache(
            PathBuf::from("/tmp/current"),
            vec![test_entry("photo.png", "/tmp/current/photo.png")],
        );
        let thumbnail_generation = state.panes.active().thumbnail_generation.current();

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
            state.panes.active().thumbnail_generation.current(),
            thumbnail_generation
        );
        assert_eq!(
            state
                .panes
                .active()
                .view
                .thumbnail_pending_key("/tmp/current/photo.png"),
            Some(&pending_key)
        );
        assert_eq!(state.panes.active().search.query, "photo");
        assert_eq!(
            state.panes.active().selection.paths,
            vec!["/tmp/current/photo.png"]
        );
        assert_eq!(
            state.panes.active().selection.anchor.as_deref(),
            Some("/tmp/current/photo.png")
        );
    }

    #[test]
    fn new_directory_load_cancels_thumbnail_pipeline_and_view_context() {
        let mut state = AppState::new(PathBuf::from("/tmp/current"), Vec::new());
        let pending_key = thumbnails::fallback_key(Path::new("/tmp/current/photo.png"), 64);
        state
            .panes
            .active_mut()
            .view
            .insert_thumbnail_pending("/tmp/current/photo.png".to_string(), pending_key);
        state.panes.active_mut().search.query = "photo".to_string();
        state.panes.active_mut().search.kind_filter = 3;
        state.panes.active_mut().selection.paths = vec!["/tmp/current/photo.png".to_string()];
        state.panes.active_mut().selection.anchor = Some("/tmp/current/photo.png".to_string());
        let thumbnail_generation = state.panes.active().thumbnail_generation.current();

        let preparation = prepare_directory_load(&mut state, false);

        assert_eq!(preparation.current_dir, PathBuf::from("/tmp/current"));
        assert!(preparation.cached_entries.is_none());
        assert!(preparation.defer_view_restore);
        assert!(state.panes.active().thumbnail_generation.current() > thumbnail_generation);
        assert!(
            !state
                .panes
                .active()
                .view
                .has_thumbnail_pending("/tmp/current/photo.png")
        );
        assert!(state.panes.active().search.query.is_empty());
        assert_eq!(state.panes.active().search.kind_filter, 0);
        assert!(state.panes.active().selection.paths.is_empty());
        assert!(state.panes.active().selection.anchor.is_none());
    }

    #[test]
    fn targeted_directory_load_updates_only_requested_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/active"), Vec::new());
        state.panes.active_mut().search.query = "active-query".to_string();
        state.panes.active_mut().selection.paths = vec!["/tmp/active/keep.txt".to_string()];
        let active_pending_key = thumbnails::fallback_key(Path::new("/tmp/active/keep.txt"), 64);
        state
            .panes
            .active_mut()
            .view
            .insert_thumbnail_pending("/tmp/active/keep.txt".to_string(), active_pending_key);
        assert!(state.panes.open_inactive(PathBuf::from("/tmp/inactive")));
        let inactive_id = state.panes.inactive().expect("inactive pane").id;
        {
            let inactive = state.panes.inactive_mut().expect("inactive pane");
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
        assert_eq!(state.panes.active().search.query, "active-query");
        assert_eq!(
            state.panes.active().selection.paths,
            vec!["/tmp/active/keep.txt"]
        );
        assert!(
            state
                .panes
                .active()
                .view
                .has_thumbnail_pending("/tmp/active/keep.txt")
        );

        let inactive = state.panes.inactive().expect("inactive pane");
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
            vec![test_entry("cached.txt", "/tmp/current/cached.txt")],
        );

        let preparation = prepare_directory_load(&mut state, false);

        assert!(preparation.cached_entries.is_some());
        assert!(!preparation.defer_view_restore);
    }

    #[test]
    fn failed_uncached_navigation_rolls_back_to_last_committed_items_path() {
        assert_eq!(
            directory_load_error_recovery(
                false,
                Path::new("/run/media/yk/missing"),
                "/home/yk",
                true,
            ),
            DirectoryLoadErrorRecovery::RollBackToItemsPath(PathBuf::from("/home/yk"))
        );
    }

    #[test]
    fn failed_cached_navigation_keeps_cached_target_model() {
        assert_eq!(
            directory_load_error_recovery(false, Path::new("/home/yk"), "/home/yk", true),
            DirectoryLoadErrorRecovery::KeepVisibleModel
        );
    }

    #[test]
    fn failed_refresh_keeps_existing_visible_model() {
        assert_eq!(
            directory_load_error_recovery(true, Path::new("/home/yk"), "/home/yk", true),
            DirectoryLoadErrorRecovery::KeepVisibleModel
        );
    }

    #[test]
    fn failed_initial_load_without_visible_model_clears_target() {
        assert_eq!(
            directory_load_error_recovery(false, Path::new("/missing"), "", false),
            DirectoryLoadErrorRecovery::ClearTarget
        );
    }

    fn test_entry(name: &str, path: &str) -> FileEntry {
        FileEntry {
            name: name.into(),
            path: path.into(),
            group: String::new().into(),
            location: String::new().into(),
            kind: "File".into(),
            size: "1 KB".into(),
            size_bytes: 1024.0,
            modified: "Today".into(),
            modified_age_days: 0,
            is_dir: false,
            thumbnail_state: 0,
            thumbnail: Image::default(),
        }
    }

    fn test_raw_entry(name: &str, path: &str) -> RawFileEntry {
        RawFileEntry {
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
        }
    }

    #[test]
    fn directory_entries_match_detects_equivalent_visible_model() {
        let current = vec![test_entry("photo.png", "/tmp/current/photo.png")];
        let incoming = vec![test_raw_entry("photo.png", "/tmp/current/photo.png")];

        assert!(directory_entries_match(&current, &incoming));
    }

    #[test]
    fn directory_entries_match_detects_visible_changes() {
        let current = vec![test_entry("photo.png", "/tmp/current/photo.png")];
        let incoming = vec![test_raw_entry("notes.txt", "/tmp/current/notes.txt")];

        assert!(!directory_entries_match(&current, &incoming));
    }
}
