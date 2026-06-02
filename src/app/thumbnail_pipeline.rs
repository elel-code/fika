use crate::FileEntry;
use crate::app::state::AppState;
use crate::fs::thumbnails;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::ops::Range;
use std::path::{Path, PathBuf};

pub(crate) const MAX_THUMBNAIL_CACHE_ENTRIES: usize = 512;
pub(crate) const MAX_THUMBNAIL_FAILURE_ENTRIES: usize = 512;

pub(crate) fn decorate_entries_with_cached_thumbnails(
    state: &AppState,
    entries: &mut [FileEntry],
    size_px: u32,
) {
    for entry in entries {
        if entry.is_dir || !thumbnails::is_thumbnail_candidate(Path::new(entry.path.as_str())) {
            continue;
        }
        let Ok(key) = thumbnails::key_for(Path::new(entry.path.as_str()), size_px) else {
            continue;
        };
        if let Some(data) = state.thumbnail_cache.get(&key) {
            entry.thumbnail = image_from_thumbnail(data);
            entry.thumbnail_state = 2;
        } else if state.thumbnail_failures.contains_key(&key) {
            entry.thumbnail_state = 0;
        } else if state
            .panes
            .active
            .view
            .has_thumbnail_pending(entry.path.as_str())
        {
            entry.thumbnail_state = 1;
        }
    }
}

pub(crate) fn prioritize_thumbnail_entries(
    entries: &[FileEntry],
    virtual_start_index: usize,
    visible_range: Range<usize>,
) -> Vec<&FileEntry> {
    let visible_start = visible_range
        .start
        .saturating_sub(virtual_start_index)
        .min(entries.len());
    let visible_end = visible_range
        .end
        .saturating_sub(virtual_start_index)
        .min(entries.len())
        .max(visible_start);
    let mut prioritized = Vec::with_capacity(entries.len());
    prioritized.extend(entries[visible_start..visible_end].iter());
    prioritized.extend(entries[..visible_start].iter());
    prioritized.extend(entries[visible_end..].iter());
    prioritized
}

pub(crate) fn path_is_in_virtual_range(state: &AppState, path_text: &str) -> bool {
    let range_start = state.panes.active.view.virtual_view.range.start;
    let range_end = state.panes.active.view.virtual_view.range.end;
    if range_start >= range_end {
        return false;
    }

    if let Some(indices) = state.panes.active.search.visible_entry_indices.as_ref() {
        let start = range_start.min(indices.len());
        let end = range_end.min(indices.len());
        if start >= end {
            return false;
        }

        return indices[start..end]
            .iter()
            .filter_map(|entry_index| state.panes.active.entries.get(*entry_index))
            .any(|entry| entry.path.as_str() == path_text);
    }

    let start = range_start.min(state.panes.active.entries.len());
    let end = range_end.min(state.panes.active.entries.len());
    if start >= end {
        return false;
    }

    state.panes.active.entries[start..end]
        .iter()
        .any(|entry| entry.path.as_str() == path_text)
}

pub(crate) fn thumbnail_schedule_candidate(
    state: &AppState,
    entry: &FileEntry,
    size_px: u32,
) -> Option<(PathBuf, thumbnails::ThumbnailKey)> {
    if entry.is_dir || entry.thumbnail_state == 2 {
        return None;
    }

    let path = PathBuf::from(entry.path.as_str());
    if !thumbnails::is_thumbnail_candidate(&path) {
        return None;
    }

    let Ok(key) = thumbnails::key_for(&path, size_px) else {
        return None;
    };
    if state.thumbnail_cache.contains_key(&key) || state.thumbnail_failures.contains_key(&key) {
        return None;
    }
    if state
        .panes
        .active
        .view
        .thumbnail_pending_key(entry.path.as_str())
        == Some(&key)
    {
        return None;
    }

    Some((path, key))
}

pub(crate) fn apply_thumbnail_load_to_state(
    state: &mut AppState,
    generation: u64,
    path_text: &str,
    load: thumbnails::ThumbnailLoad,
) -> bool {
    if !state
        .panes
        .active
        .thumbnail_generation
        .is_current(generation)
    {
        remove_matching_thumbnail_pending(state, path_text, &load.key);
        return false;
    }

    remove_matching_thumbnail_pending(state, path_text, &load.key);
    let freedesktop_cache_paths = load.cache_paths.as_ref();

    match load.data {
        Ok(data) => {
            let _cache_path = freedesktop_cache_paths.map(|paths| &paths.thumbnail_path);
            remove_thumbnail_failure(state, &load.key);
            insert_thumbnail_cache_with_limit(state, load.key, data);
        }
        Err(err) => {
            let error = if let Some(paths) = freedesktop_cache_paths {
                format!(
                    "{}; fail marker path {}",
                    err,
                    paths.fail_marker_path.display()
                )
            } else {
                err.to_string()
            };
            insert_thumbnail_failure_with_limit(state, load.key, error);
        }
    }

    path_is_in_virtual_range(state, path_text)
}

pub(crate) fn remove_matching_thumbnail_pending(
    state: &mut AppState,
    path_text: &str,
    key: &thumbnails::ThumbnailKey,
) {
    state
        .panes
        .active
        .view
        .remove_matching_thumbnail_pending(path_text, key);
}

pub(crate) fn remove_thumbnail_failure(state: &mut AppState, key: &thumbnails::ThumbnailKey) {
    state.thumbnail_failures.remove(key);
    state.thumbnail_failure_order.retain(|cached| cached != key);
}

pub(crate) fn insert_thumbnail_cache_with_limit(
    state: &mut AppState,
    key: thumbnails::ThumbnailKey,
    data: thumbnails::ThumbnailData,
) {
    state.thumbnail_cache_order.retain(|cached| cached != &key);
    state.thumbnail_cache.insert(key.clone(), data);
    state.thumbnail_cache_order.push_back(key);

    while state.thumbnail_cache_order.len() > MAX_THUMBNAIL_CACHE_ENTRIES {
        if let Some(oldest) = state.thumbnail_cache_order.pop_front() {
            state.thumbnail_cache.remove(&oldest);
        }
    }
}

pub(crate) fn insert_thumbnail_failure_with_limit(
    state: &mut AppState,
    key: thumbnails::ThumbnailKey,
    error: String,
) {
    state
        .thumbnail_failure_order
        .retain(|cached| cached != &key);
    state.thumbnail_failures.insert(key.clone(), error);
    state.thumbnail_failure_order.push_back(key);

    while state.thumbnail_failure_order.len() > MAX_THUMBNAIL_FAILURE_ENTRIES {
        if let Some(oldest) = state.thumbnail_failure_order.pop_front() {
            state.thumbnail_failures.remove(&oldest);
        }
    }
}

fn image_from_thumbnail(data: &thumbnails::ThumbnailData) -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(data.width, data.height);
    buffer.make_mut_bytes().copy_from_slice(&data.rgba);
    Image::from_rgba8(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use std::io;

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

    #[test]
    fn thumbnail_cache_evicts_oldest_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        for index in 0..(MAX_THUMBNAIL_CACHE_ENTRIES + 3) {
            insert_thumbnail_cache_with_limit(
                &mut state,
                thumbnails::fallback_key(Path::new(&format!("/tmp/{index}.png")), 64),
                thumbnails::ThumbnailData {
                    width: 1,
                    height: 1,
                    rgba: vec![0, 0, 0, 0],
                },
            );
        }

        assert_eq!(state.thumbnail_cache.len(), MAX_THUMBNAIL_CACHE_ENTRIES);
        assert_eq!(
            state.thumbnail_cache_order.len(),
            MAX_THUMBNAIL_CACHE_ENTRIES
        );
        assert!(
            !state
                .thumbnail_cache
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/0.png"), 64))
        );
        assert!(
            state
                .thumbnail_cache
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/3.png"), 64))
        );
    }

    #[test]
    fn thumbnail_cache_refreshes_existing_entry_order() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let first = thumbnails::fallback_key(Path::new("/tmp/first.png"), 64);
        let second = thumbnails::fallback_key(Path::new("/tmp/second.png"), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 0],
        };

        insert_thumbnail_cache_with_limit(&mut state, first.clone(), data.clone());
        insert_thumbnail_cache_with_limit(&mut state, second.clone(), data.clone());
        insert_thumbnail_cache_with_limit(&mut state, first.clone(), data);

        assert_eq!(state.thumbnail_cache_order.pop_back(), Some(first));
        assert_eq!(state.thumbnail_cache_order.pop_front(), Some(second));
    }

    #[test]
    fn thumbnail_failure_cache_evicts_oldest_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        for index in 0..(MAX_THUMBNAIL_FAILURE_ENTRIES + 3) {
            insert_thumbnail_failure_with_limit(
                &mut state,
                thumbnails::fallback_key(Path::new(&format!("/tmp/{index}.png")), 64),
                "decode failed".to_string(),
            );
        }

        assert_eq!(
            state.thumbnail_failures.len(),
            MAX_THUMBNAIL_FAILURE_ENTRIES
        );
        assert_eq!(
            state.thumbnail_failure_order.len(),
            MAX_THUMBNAIL_FAILURE_ENTRIES
        );
        assert!(
            !state
                .thumbnail_failures
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/0.png"), 64))
        );
        assert!(
            state
                .thumbnail_failures
                .contains_key(&thumbnails::fallback_key(Path::new("/tmp/3.png"), 64))
        );
    }

    #[test]
    fn thumbnail_failure_cache_refreshes_existing_entry_order() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let first = thumbnails::fallback_key(Path::new("/tmp/first.png"), 64);
        let second = thumbnails::fallback_key(Path::new("/tmp/second.png"), 64);

        insert_thumbnail_failure_with_limit(&mut state, first.clone(), "first".to_string());
        insert_thumbnail_failure_with_limit(&mut state, second.clone(), "second".to_string());
        insert_thumbnail_failure_with_limit(&mut state, first.clone(), "first again".to_string());

        assert_eq!(state.thumbnail_failure_order.pop_back(), Some(first));
        assert_eq!(state.thumbnail_failure_order.pop_front(), Some(second));
    }

    #[test]
    fn thumbnail_schedule_skips_failed_key_until_file_changes() {
        let temp_dir =
            std::env::temp_dir().join(format!("fika-thumbnail-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let image_path = temp_dir.join("broken.png");
        std::fs::write(&image_path, b"not an image").unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let entry = test_entry("broken.png", image_path.to_str().unwrap());
        let key = thumbnails::key_for(&image_path, 64).unwrap();
        assert!(thumbnail_schedule_candidate(&state, &entry, 64).is_some());

        insert_thumbnail_failure_with_limit(&mut state, key.clone(), "decode failed".to_string());
        assert!(thumbnail_schedule_candidate(&state, &entry, 64).is_none());

        remove_thumbnail_failure(&mut state, &key);
        assert!(thumbnail_schedule_candidate(&state, &entry, 64).is_some());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn successful_thumbnail_result_removes_failure_marker() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let key = thumbnails::fallback_key(Path::new("/tmp/photo.png"), 64);

        insert_thumbnail_failure_with_limit(&mut state, key.clone(), "decode failed".to_string());
        assert!(state.thumbnail_failures.contains_key(&key));

        remove_thumbnail_failure(&mut state, &key);
        assert!(!state.thumbnail_failures.contains_key(&key));
        assert!(!state.thumbnail_failure_order.contains(&key));
    }

    #[test]
    fn stale_thumbnail_result_only_clears_matching_pending_key() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let path = "/tmp/photo.png";
        let old_key = thumbnails::fallback_key(Path::new(path), 64);
        let new_key = thumbnails::fallback_key(Path::new(path), 128);

        state
            .panes
            .active
            .view
            .insert_thumbnail_pending(path.to_string(), new_key.clone());
        remove_matching_thumbnail_pending(&mut state, path, &old_key);
        assert_eq!(
            state.panes.active.view.thumbnail_pending_key(path),
            Some(&new_key)
        );

        remove_matching_thumbnail_pending(&mut state, path, &new_key);
        assert!(!state.panes.active.view.has_thumbnail_pending(path));
    }

    #[test]
    fn thumbnail_success_result_updates_cache_without_mutating_full_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let generation = state.panes.active.thumbnail_generation.current();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };

        state.panes.active.entries = vec![test_entry("photo.png", path)];
        state.panes.active.view.virtual_view.range = 0..1;
        state
            .panes
            .active
            .view
            .insert_thumbnail_pending(path.to_string(), key.clone());
        insert_thumbnail_failure_with_limit(&mut state, key.clone(), "decode failed".to_string());

        let should_refresh = apply_thumbnail_load_to_state(
            &mut state,
            generation,
            path,
            thumbnails::ThumbnailLoad {
                path: PathBuf::from(path),
                key: key.clone(),
                cache_paths: None,
                data: Ok(data),
            },
        );

        assert!(should_refresh);
        assert!(!state.panes.active.view.has_thumbnail_pending(path));
        assert!(state.thumbnail_cache.contains_key(&key));
        assert!(!state.thumbnail_failures.contains_key(&key));
        assert_eq!(state.panes.active.entries[0].thumbnail_state, 0);
    }

    #[test]
    fn thumbnail_failure_result_updates_failure_cache_without_mutating_full_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let generation = state.panes.active.thumbnail_generation.current();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);

        let mut entry = test_entry("photo.png", path);
        entry.thumbnail_state = 1;
        state.panes.active.entries = vec![entry];
        state.panes.active.view.virtual_view.range = 0..1;
        state
            .panes
            .active
            .view
            .insert_thumbnail_pending(path.to_string(), key.clone());

        let should_refresh = apply_thumbnail_load_to_state(
            &mut state,
            generation,
            path,
            thumbnails::ThumbnailLoad {
                path: PathBuf::from(path),
                key: key.clone(),
                cache_paths: None,
                data: Err(io::Error::other("decode failed")),
            },
        );

        assert!(should_refresh);
        assert!(!state.panes.active.view.has_thumbnail_pending(path));
        assert!(state.thumbnail_failures.contains_key(&key));
        assert_eq!(state.panes.active.entries[0].thumbnail_state, 1);
    }

    #[test]
    fn stale_thumbnail_result_does_not_update_thumbnail_caches() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let stale_generation = state.panes.active.thumbnail_generation.current();
        state.panes.active.thumbnail_generation.next();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 0],
        };
        state
            .panes
            .active
            .view
            .insert_thumbnail_pending(path.to_string(), key.clone());

        let should_refresh = apply_thumbnail_load_to_state(
            &mut state,
            stale_generation,
            path,
            thumbnails::ThumbnailLoad {
                path: PathBuf::from(path),
                key: key.clone(),
                cache_paths: None,
                data: Ok(data),
            },
        );

        assert!(!should_refresh);
        assert!(!state.panes.active.view.has_thumbnail_pending(path));
        assert!(!state.thumbnail_cache.contains_key(&key));
        assert!(!state.thumbnail_failures.contains_key(&key));
    }

    #[test]
    fn thumbnail_priority_schedules_visible_entries_before_overscan() {
        let entries = (8..20)
            .map(|index| test_entry(&format!("item-{index}.png"), &format!("/tmp/{index}.png")))
            .collect::<Vec<_>>();

        let prioritized = prioritize_thumbnail_entries(&entries, 8, 12..16)
            .into_iter()
            .map(|entry| entry.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            prioritized,
            vec![
                "item-12.png".to_string(),
                "item-13.png".to_string(),
                "item-14.png".to_string(),
                "item-15.png".to_string(),
                "item-8.png".to_string(),
                "item-9.png".to_string(),
                "item-10.png".to_string(),
                "item-11.png".to_string(),
                "item-16.png".to_string(),
                "item-17.png".to_string(),
                "item-18.png".to_string(),
                "item-19.png".to_string(),
            ]
        );
    }

    #[test]
    fn virtual_range_path_lookup_uses_identity_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.active.entries = (0..6)
            .map(|index| test_entry(&format!("{index}.png"), &format!("/tmp/{index}.png")))
            .collect();
        state.panes.active.view.virtual_view.range = 2..5;

        assert!(path_is_in_virtual_range(&state, "/tmp/2.png"));
        assert!(path_is_in_virtual_range(&state, "/tmp/4.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/1.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/5.png"));
    }

    #[test]
    fn virtual_range_path_lookup_uses_filtered_visible_indices() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.active.entries = vec![
            test_entry("alpha.png", "/tmp/alpha.png"),
            test_entry("skip.log", "/tmp/skip.log"),
            test_entry("beta.png", "/tmp/beta.png"),
            test_entry("gamma.png", "/tmp/gamma.png"),
        ];
        state.panes.active.search.visible_entry_indices = Some(vec![0, 2, 3]);
        state.panes.active.view.virtual_view.range = 1..3;

        assert!(path_is_in_virtual_range(&state, "/tmp/beta.png"));
        assert!(path_is_in_virtual_range(&state, "/tmp/gamma.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/skip.log"));
    }

    #[test]
    fn virtual_range_path_lookup_rejects_empty_or_stale_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.active.entries = vec![test_entry("alpha.png", "/tmp/alpha.png")];
        state.panes.active.view.virtual_view.range = 0..0;
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));

        state.panes.active.view.virtual_view.range = 9..12;
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));
    }
}
