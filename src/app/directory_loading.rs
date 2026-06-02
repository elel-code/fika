use crate::FileEntry;
use crate::app::search_ui::{cancel_active_search, reset_search_state};
use crate::app::state::AppState;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct DirectoryLoadPreparation {
    pub(crate) current_dir: PathBuf,
    pub(crate) generation: u64,
    pub(crate) cached_entries: Option<Vec<FileEntry>>,
}

pub(crate) fn prepare_directory_load(
    state: &mut AppState,
    preserve_view: bool,
) -> DirectoryLoadPreparation {
    cancel_active_search(state);
    let generation = state.load_generation.next();
    state.open_generation.next();
    state.search_generation.next();
    if !preserve_view {
        state.thumbnail_generation.next();
        state.thumbnail_pending.clear();
        reset_search_state(state);
        state.selected_paths.clear();
        state.selection_anchor = None;
    }
    let current_dir = state.current_dir.clone();
    let cached_entries = state.cached_directory_entries(&current_dir);

    DirectoryLoadPreparation {
        current_dir,
        generation,
        cached_entries,
    }
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
            .thumbnail_pending
            .insert("/tmp/current/photo.png".to_string(), pending_key.clone());
        state.search_query = "photo".to_string();
        state.selected_paths = vec!["/tmp/current/photo.png".to_string()];
        state.selection_anchor = Some("/tmp/current/photo.png".to_string());
        state.insert_directory_cache(
            PathBuf::from("/tmp/current"),
            vec![test_entry("photo.png", "/tmp/current/photo.png")],
        );
        let thumbnail_generation = state.thumbnail_generation.current();

        let preparation = prepare_directory_load(&mut state, true);

        assert_eq!(preparation.current_dir, PathBuf::from("/tmp/current"));
        assert_eq!(
            preparation
                .cached_entries
                .as_ref()
                .map(|entries| entries.len()),
            Some(1)
        );
        assert_eq!(state.thumbnail_generation.current(), thumbnail_generation);
        assert_eq!(
            state.thumbnail_pending.get("/tmp/current/photo.png"),
            Some(&pending_key)
        );
        assert_eq!(state.search_query, "photo");
        assert_eq!(state.selected_paths, vec!["/tmp/current/photo.png"]);
        assert_eq!(
            state.selection_anchor.as_deref(),
            Some("/tmp/current/photo.png")
        );
    }

    #[test]
    fn new_directory_load_cancels_thumbnail_pipeline_and_view_context() {
        let mut state = AppState::new(PathBuf::from("/tmp/current"), Vec::new());
        let pending_key = thumbnails::fallback_key(Path::new("/tmp/current/photo.png"), 64);
        state
            .thumbnail_pending
            .insert("/tmp/current/photo.png".to_string(), pending_key);
        state.search_query = "photo".to_string();
        state.search_kind_filter = 3;
        state.selected_paths = vec!["/tmp/current/photo.png".to_string()];
        state.selection_anchor = Some("/tmp/current/photo.png".to_string());
        let thumbnail_generation = state.thumbnail_generation.current();

        let preparation = prepare_directory_load(&mut state, false);

        assert_eq!(preparation.current_dir, PathBuf::from("/tmp/current"));
        assert!(preparation.cached_entries.is_none());
        assert!(state.thumbnail_generation.current() > thumbnail_generation);
        assert!(state.thumbnail_pending.is_empty());
        assert!(state.search_query.is_empty());
        assert_eq!(state.search_kind_filter, 0);
        assert!(state.selected_paths.is_empty());
        assert!(state.selection_anchor.is_none());
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
}
