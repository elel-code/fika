use std::path::Path;
use std::sync::Arc;

use super::{FileIconCache, FileIconSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileIconRoleSnapshot {
    pub(crate) icon: FileIconSnapshot,
    pub(crate) icon_name_to_store: Option<Arc<str>>,
}

pub(crate) fn file_icon_snapshot_for_model_role(
    cache: &mut FileIconCache,
    stored_icon_name: Option<Arc<str>>,
    path: &Path,
    is_dir: bool,
    metadata_complete: bool,
    size_bytes: u64,
    mime_type: Option<Arc<str>>,
    mime_magic_checked: bool,
    icon_size: f32,
) -> FileIconRoleSnapshot {
    if let Some(icon_name) = stored_icon_name {
        let icon = cache.icon_for_name_role(icon_name.as_ref(), path, is_dir, mime_type, icon_size);
        return FileIconRoleSnapshot {
            icon,
            icon_name_to_store: None,
        };
    }

    let icon_name = cache.icon_name_for(path, is_dir, mime_type.clone());
    let icon = cache.icon_for_name_role(
        icon_name.as_ref(),
        path,
        is_dir,
        mime_type.clone(),
        icon_size,
    );
    let icon_name_to_store = file_icon_role_is_final(
        metadata_complete,
        is_dir,
        size_bytes,
        mime_type.as_deref(),
        mime_magic_checked,
    )
    .then_some(icon_name);

    FileIconRoleSnapshot {
        icon,
        icon_name_to_store,
    }
}

pub(crate) fn file_icon_role_is_final(
    metadata_complete: bool,
    is_dir: bool,
    size_bytes: u64,
    mime_type: Option<&str>,
    mime_magic_checked: bool,
) -> bool {
    metadata_complete
        && !fika_core::mime_magic_resolution_required(
            is_dir,
            size_bytes,
            mime_type,
            mime_magic_checked,
        )
}

pub(crate) fn finish_metadata_role_results_with_icon_roles(
    panes: &mut fika_core::PaneController,
    cache: &mut FileIconCache,
    results: Vec<fika_core::MetadataRoleResult>,
) -> bool {
    let mut changed = false;
    let mut icon_role_updates = Vec::new();

    for result in results {
        let pane_id = result.pane_id;
        let generation = result.generation;
        let item_id = result.item_id;
        let Some(pane) = panes.pane_mut(result.pane_id) else {
            continue;
        };
        if pane.generation != result.generation {
            continue;
        }
        if fika_core::apply_metadata_role_result_to_model(&mut pane.model, result) {
            changed = true;
            if let Some(index) = pane.model.index_of_id(item_id)
                && let Some(entry) = pane.model.get(index)
                && let Some(path) = pane.model.path_for_index(index)
            {
                icon_role_updates.push((
                    pane_id,
                    generation,
                    item_id,
                    path,
                    entry.is_dir,
                    entry.effective_mime_type_cloned(),
                ));
            }
        }
    }

    for (pane_id, generation, item_id, path, is_dir, mime_type) in icon_role_updates {
        let icon_name = cache.icon_name_for(&path, is_dir, mime_type);
        let Some(pane) = panes.pane_mut(pane_id) else {
            continue;
        };
        if pane.generation == generation
            && !pane
                .model
                .set_icon_name_role(item_id, Some(icon_name))
                .is_empty()
        {
            changed = true;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENERIC_BINARY_MIME: &str = "application/octet-stream";

    #[test]
    fn incomplete_generic_file_icon_is_widget_local_until_role_resolution() {
        let mut cache = FileIconCache::default();

        let snapshot = file_icon_snapshot_for_model_role(
            &mut cache,
            None,
            Path::new("settings.conf"),
            false,
            false,
            12,
            Some(Arc::from(GENERIC_BINARY_MIME)),
            false,
            48.0,
        );

        assert_eq!(snapshot.icon.icon_name.as_ref(), "application-octet-stream");
        assert_eq!(snapshot.icon_name_to_store, None);
    }

    #[test]
    fn complete_file_without_icon_role_uses_fast_mime_icon_until_role_update() {
        let mut cache = FileIconCache::default();

        let snapshot = file_icon_snapshot_for_model_role(
            &mut cache,
            None,
            Path::new("notes.txt"),
            false,
            true,
            12,
            Some(Arc::from("text/plain")),
            true,
            48.0,
        );

        assert_eq!(snapshot.icon.icon_name.as_ref(), "text-plain");
        assert_eq!(snapshot.icon_name_to_store.as_deref(), Some("text-plain"));
    }

    #[test]
    fn incomplete_metadata_with_known_mime_uses_fast_icon_until_role_update() {
        let mut cache = FileIconCache::default();

        let snapshot = file_icon_snapshot_for_model_role(
            &mut cache,
            None,
            Path::new("notes.txt"),
            false,
            false,
            12,
            Some(Arc::from("text/plain")),
            true,
            48.0,
        );

        assert_eq!(snapshot.icon.icon_name.as_ref(), "text-plain");
        assert_eq!(snapshot.icon_name_to_store, None);
    }

    #[test]
    fn directory_without_icon_role_uses_widget_local_folder_icon() {
        let mut cache = FileIconCache::default();

        let snapshot = file_icon_snapshot_for_model_role(
            &mut cache,
            None,
            Path::new("Documents"),
            true,
            false,
            0,
            Some(Arc::from("inode/directory")),
            true,
            48.0,
        );

        assert_eq!(snapshot.icon.icon_name.as_ref(), "folder");
        assert_eq!(snapshot.icon_name_to_store, None);
    }

    #[test]
    fn stored_icon_name_role_is_used_without_recomputing_from_mime() {
        let mut cache = FileIconCache::default();

        let snapshot = file_icon_snapshot_for_model_role(
            &mut cache,
            Some(Arc::from("text-x-source")),
            Path::new("notes.txt"),
            false,
            true,
            12,
            Some(Arc::from("text/plain")),
            true,
            48.0,
        );

        assert_eq!(snapshot.icon.icon_name.as_ref(), "text-x-source");
        assert_eq!(snapshot.icon_name_to_store, None);
    }

    #[test]
    fn stored_icon_name_role_is_used_while_metadata_refresh_is_pending() {
        let mut cache = FileIconCache::default();

        let snapshot = file_icon_snapshot_for_model_role(
            &mut cache,
            Some(Arc::from("text-x-source")),
            Path::new("notes.txt"),
            false,
            false,
            12,
            Some(Arc::from("text/plain")),
            true,
            48.0,
        );

        assert_eq!(snapshot.icon.icon_name.as_ref(), "text-x-source");
        assert_eq!(snapshot.icon_name_to_store, None);
    }

    #[test]
    fn metadata_role_result_updates_final_icon_role_without_clearing_thumbnail() {
        let directory = std::path::PathBuf::from("/tmp/fika-icon-role-metadata");
        let mut panes = fika_core::PaneController::new(directory.clone());
        let pane_id = panes.focused().unwrap();
        panes.pane_mut(pane_id).unwrap().model.replace_listing(
            directory.clone(),
            Arc::new(vec![fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("payload"),
                name_width_units: 7,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from(GENERIC_BINARY_MIME)),
                mime_magic_checked: false,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            })]),
        );
        let generation = panes.pane(pane_id).unwrap().generation;
        let item_id = panes.pane(pane_id).unwrap().model.entries()[0].id;
        let thumbnail_path = std::path::PathBuf::from("/tmp/fika-thumbnail/normal/payload.png");
        panes
            .pane_mut(pane_id)
            .unwrap()
            .model
            .set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

        let mut cache = FileIconCache::default();
        let changed = finish_metadata_role_results_with_icon_roles(
            &mut panes,
            &mut cache,
            vec![fika_core::MetadataRoleResult {
                pane_id,
                generation,
                item_id,
                path: directory.join("payload"),
                role: Some(fika_core::EntryMetadataRole {
                    size_bytes: 12,
                    modified_secs: Some(42),
                    mime_type: Some(Arc::from("text/plain")),
                    mime_magic_checked: true,
                }),
            }],
        );

        let entry = &panes.pane(pane_id).unwrap().model.entries()[0];
        assert!(changed);
        assert_eq!(
            entry.effective_mime_type().map(Arc::as_ref),
            Some("text/plain")
        );
        assert_eq!(entry.icon_name.as_deref(), Some("text-plain"));
        assert_eq!(
            entry.thumbnail_path.as_deref(),
            Some(thumbnail_path.as_path())
        );
    }
}
