use crate::ItemViewEntry;
use crate::app::item_view_model::ItemViewModelEntry;
use crate::app::item_view_renderer::ItemViewMediaSource;
use crate::app::model_update::ItemViewRowToken;
use crate::app::state::AppState;
use crate::fs::thumbnails;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, SharedString};
use std::path::{Path, PathBuf};

pub(crate) const MAX_THUMBNAIL_CACHE_ENTRIES: usize = 512;
pub(crate) const MAX_THUMBNAIL_FAILURE_ENTRIES: usize = 512;
pub(crate) const MAX_THUMBNAIL_JOBS_PER_VIEW_SYNC: usize = 96;
pub(crate) const THUMBNAIL_STATE_NOT_CANDIDATE: i32 = -1;
pub(crate) const THUMBNAIL_STATE_EMPTY: i32 = 0;
pub(crate) const THUMBNAIL_STATE_PENDING: i32 = 1;
pub(crate) const THUMBNAIL_STATE_LOADED: i32 = 2;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThumbnailScheduleEntry {
    path: SharedString,
    is_dir: bool,
    thumbnail_state: i32,
    media_token: i32,
}

impl ThumbnailScheduleEntry {
    pub(crate) fn from_row_token(token: &ItemViewRowToken) -> Self {
        Self {
            path: token.path_shared(),
            is_dir: token.is_dir(),
            thumbnail_state: token.thumbnail_state(),
            media_token: token.media_token(),
        }
    }
}

pub(crate) trait ThumbnailScheduleRow {
    fn path(&self) -> &str;
    fn is_dir(&self) -> bool;
    fn thumbnail_state(&self) -> i32;
    fn media_token(&self) -> i32;
}

impl ThumbnailScheduleRow for ItemViewEntry {
    fn path(&self) -> &str {
        self.path.as_str()
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn thumbnail_state(&self) -> i32 {
        self.thumbnail_state
    }

    fn media_token(&self) -> i32 {
        self.media_token
    }
}

impl ThumbnailScheduleRow for ItemViewRowToken {
    fn path(&self) -> &str {
        self.path()
    }

    fn is_dir(&self) -> bool {
        self.is_dir()
    }

    fn thumbnail_state(&self) -> i32 {
        self.thumbnail_state()
    }

    fn media_token(&self) -> i32 {
        self.media_token()
    }
}

impl ThumbnailScheduleRow for ThumbnailScheduleEntry {
    fn path(&self) -> &str {
        self.path.as_str()
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn thumbnail_state(&self) -> i32 {
        self.thumbnail_state
    }

    fn media_token(&self) -> i32 {
        self.media_token
    }
}

pub(crate) fn decorate_entries_with_cached_thumbnails_for_pane(
    state: &AppState,
    pane_id: u64,
    entries: &mut [ItemViewEntry],
    size_px: u32,
) -> Vec<ItemViewMediaSource> {
    let Some(pane) = state.panes.pane_by_id(pane_id) else {
        return Vec::new();
    };

    let mut media_entries = Vec::new();
    for (row, entry) in entries.iter_mut().enumerate() {
        if entry.is_dir {
            continue;
        }
        if !thumbnails::is_thumbnail_candidate(Path::new(entry.path.as_str())) {
            entry.thumbnail_state = THUMBNAIL_STATE_NOT_CANDIDATE;
            continue;
        }
        let Ok(key) = thumbnails::key_for(Path::new(entry.path.as_str()), size_px) else {
            continue;
        };
        if let Some(data) = state.thumbnail_cache.get(&key) {
            media_entries.push(ItemViewMediaSource {
                slice_index: row as i32,
                media: image_from_thumbnail(data),
                x: 0.0,
                y: 0.0,
            });
            entry.media_token = key.item_view_media_token();
            entry.thumbnail_state = THUMBNAIL_STATE_LOADED;
        } else if state.thumbnail_failures.contains_key(&key) {
            entry.thumbnail_state = THUMBNAIL_STATE_EMPTY;
        } else if pane.view.has_thumbnail_pending(entry.path.as_str()) {
            entry.thumbnail_state = THUMBNAIL_STATE_PENDING;
        }
    }
    media_entries
}

#[cfg(test)]
pub(crate) fn path_is_in_virtual_range(state: &AppState, path_text: &str) -> bool {
    path_is_in_virtual_range_for_pane(state, state.panes.focused().id, path_text)
}

pub(crate) fn path_is_in_virtual_range_for_pane(
    state: &AppState,
    pane_id: u64,
    path_text: &str,
) -> bool {
    let Some(pane) = state.panes.pane_by_id(pane_id) else {
        return false;
    };
    let range_start = pane.view.virtual_view.range.start;
    let range_end = pane.view.virtual_view.range.end;
    if range_start >= range_end {
        return false;
    }

    if let Some(indices) = pane.search.visible_entry_indices.as_ref() {
        let start = range_start.min(indices.len());
        let end = range_end.min(indices.len());
        if start >= end {
            return false;
        }

        return indices[start..end]
            .iter()
            .any(|&entry_index| pane.entries[entry_index].model_path() == path_text);
    }

    let start = range_start.min(pane.entries.len());
    let end = range_end.min(pane.entries.len());
    if start >= end {
        return false;
    }

    pane.entries[start..end]
        .iter()
        .any(|entry| entry.model_path() == path_text)
}

#[cfg(test)]
pub(crate) fn thumbnail_schedule_candidate(
    state: &AppState,
    entry: &ItemViewEntry,
    size_px: u32,
) -> Option<(PathBuf, thumbnails::ThumbnailKey)> {
    thumbnail_schedule_candidate_for_pane(state, state.panes.focused().id, entry, size_px)
}

pub(crate) fn thumbnail_schedule_candidate_for_pane<T: ThumbnailScheduleRow + ?Sized>(
    state: &AppState,
    pane_id: u64,
    entry: &T,
    size_px: u32,
) -> Option<(PathBuf, thumbnails::ThumbnailKey)> {
    if entry.is_dir() {
        return None;
    }
    if entry.thumbnail_state() == THUMBNAIL_STATE_NOT_CANDIDATE {
        return None;
    }

    let path = PathBuf::from(entry.path());
    if !thumbnails::is_thumbnail_candidate(&path) {
        return None;
    }

    let Ok(key) = thumbnails::key_for(&path, size_px) else {
        return None;
    };
    if entry.thumbnail_state() == THUMBNAIL_STATE_LOADED
        && entry.media_token() == key.item_view_media_token()
    {
        return None;
    }
    if state.thumbnail_cache.contains_key(&key) || state.thumbnail_failures.contains_key(&key) {
        return None;
    }
    let pane = state.panes.pane_by_id(pane_id)?;
    if pane.view.thumbnail_pending_key(entry.path()) == Some(&key) {
        return None;
    }

    Some((path, key))
}

#[cfg(test)]
pub(crate) fn thumbnail_schedule_batch<'a, T, I>(
    state: &mut AppState,
    entries: I,
    size_px: u32,
) -> Vec<PathBuf>
where
    T: ThumbnailScheduleRow + ?Sized + 'a,
    I: IntoIterator<Item = &'a T>,
{
    let pane_id = state.panes.focused().id;
    thumbnail_schedule_batch_for_pane(state, pane_id, entries, size_px)
}

pub(crate) fn thumbnail_schedule_batch_for_pane<'a, T, I>(
    state: &mut AppState,
    pane_id: u64,
    entries: I,
    size_px: u32,
) -> Vec<PathBuf>
where
    T: ThumbnailScheduleRow + ?Sized + 'a,
    I: IntoIterator<Item = &'a T>,
{
    let mut paths = Vec::new();
    for entry in entries {
        if paths.len() >= MAX_THUMBNAIL_JOBS_PER_VIEW_SYNC {
            break;
        }

        let Some((path, key)) =
            thumbnail_schedule_candidate_for_pane(state, pane_id, entry, size_px)
        else {
            continue;
        };
        let Some(pane) = state.panes.pane_mut_by_id(pane_id) else {
            continue;
        };
        pane.view
            .insert_thumbnail_pending(entry.path().to_string(), key);
        paths.push(path);
    }
    paths
}

#[cfg(test)]
pub(crate) fn apply_thumbnail_load_to_state(
    state: &mut AppState,
    generation: u64,
    path_text: &str,
    load: thumbnails::ThumbnailLoad,
) -> bool {
    apply_thumbnail_load_to_state_for_pane(
        state,
        state.panes.focused().id,
        generation,
        path_text,
        load,
    )
}

pub(crate) fn apply_thumbnail_load_to_state_for_pane(
    state: &mut AppState,
    pane_id: u64,
    generation: u64,
    path_text: &str,
    load: thumbnails::ThumbnailLoad,
) -> bool {
    let Some(pane) = state.panes.pane_by_id(pane_id) else {
        return false;
    };
    if !pane.thumbnail_generation.is_current(generation) {
        remove_matching_thumbnail_pending_for_pane(state, pane_id, path_text, &load.key);
        return false;
    }

    remove_matching_thumbnail_pending_for_pane(state, pane_id, path_text, &load.key);
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

    path_is_in_virtual_range_for_pane(state, pane_id, path_text)
}

#[cfg(test)]
pub(crate) fn remove_matching_thumbnail_pending(
    state: &mut AppState,
    path_text: &str,
    key: &thumbnails::ThumbnailKey,
) {
    remove_matching_thumbnail_pending_for_pane(state, state.panes.focused().id, path_text, key);
}

pub(crate) fn remove_matching_thumbnail_pending_for_pane(
    state: &mut AppState,
    pane_id: u64,
    path_text: &str,
    key: &thumbnails::ThumbnailKey,
) {
    if let Some(pane) = state.panes.pane_mut_by_id(pane_id) {
        pane.view.remove_matching_thumbnail_pending(path_text, key);
    }
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
    use crate::FileEntry;
    use crate::app::state::AppState;
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn test_entry(name: &str, path: &str) -> ItemViewEntry {
        ItemViewEntry {
            name: name.into(),
            path: path.into(),
            is_dir: false,
            thumbnail_state: THUMBNAIL_STATE_EMPTY,
            media_token: 0,
        }
    }

    fn business_entry(name: &str, path: &str) -> FileEntry {
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
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fika-thumbnail-pipeline-{}-{name}-{counter}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
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
            .focused_mut()
            .view
            .insert_thumbnail_pending(path.to_string(), new_key.clone());
        remove_matching_thumbnail_pending(&mut state, path, &old_key);
        assert_eq!(
            state.panes.focused().view.thumbnail_pending_key(path),
            Some(&new_key)
        );

        remove_matching_thumbnail_pending(&mut state, path, &new_key);
        assert!(!state.panes.focused().view.has_thumbnail_pending(path));
    }

    #[test]
    fn thumbnail_success_result_updates_cache_without_mutating_full_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let generation = state.panes.focused().thumbnail_generation.current();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };

        state
            .panes
            .focused_mut()
            .set_file_entries(vec![business_entry("photo.png", path)]);
        state.panes.focused_mut().view.virtual_view.range = 0..1;
        state
            .panes
            .focused_mut()
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
        assert!(!state.panes.focused().view.has_thumbnail_pending(path));
        assert!(state.thumbnail_cache.contains_key(&key));
        assert!(!state.thumbnail_failures.contains_key(&key));
        assert_eq!(state.panes.focused().entries[0].path, path);
        assert_eq!(
            state.panes.focused().entries[0].model_to_file_entry(),
            business_entry("photo.png", path)
        );
    }

    #[test]
    fn thumbnail_failure_result_updates_failure_cache_without_mutating_full_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let generation = state.panes.focused().thumbnail_generation.current();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);

        state
            .panes
            .focused_mut()
            .set_file_entries(vec![business_entry("photo.png", path)]);
        state.panes.focused_mut().view.virtual_view.range = 0..1;
        state
            .panes
            .focused_mut()
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
        assert!(!state.panes.focused().view.has_thumbnail_pending(path));
        assert!(state.thumbnail_failures.contains_key(&key));
        assert_eq!(state.panes.focused().entries[0].path, path);
        assert_eq!(
            state.panes.focused().entries[0].model_to_file_entry(),
            business_entry("photo.png", path)
        );
    }

    #[test]
    fn stale_thumbnail_result_does_not_update_thumbnail_caches() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let stale_generation = state.panes.focused().thumbnail_generation.current();
        state.panes.focused_mut().thumbnail_generation.next();
        let path = "/tmp/photo.png";
        let key = thumbnails::fallback_key(Path::new(path), 64);
        let data = thumbnails::ThumbnailData {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 0],
        };
        state
            .panes
            .focused_mut()
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
        assert!(!state.panes.focused().view.has_thumbnail_pending(path));
        assert!(!state.thumbnail_cache.contains_key(&key));
        assert!(!state.thumbnail_failures.contains_key(&key));
    }

    #[test]
    fn thumbnail_schedule_entries_can_derive_from_row_tokens_without_images() {
        let temp_dir = temp_test_dir("row-token-schedule");
        let entries = (0..6)
            .map(|index| {
                let path = temp_dir.join(format!("item-{index}.png"));
                std::fs::write(&path, b"not a real image").unwrap();
                test_entry(&format!("item-{index}.png"), path.to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let tokens = entries
            .iter()
            .map(ItemViewRowToken::from_entry)
            .map(|token| ThumbnailScheduleEntry::from_row_token(&token))
            .collect::<Vec<_>>();

        let token_paths = tokens
            .iter()
            .map(|entry| entry.path().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            token_paths,
            vec![
                temp_dir.join("item-0.png").to_string_lossy().to_string(),
                temp_dir.join("item-1.png").to_string_lossy().to_string(),
                temp_dir.join("item-2.png").to_string_lossy().to_string(),
                temp_dir.join("item-3.png").to_string_lossy().to_string(),
                temp_dir.join("item-4.png").to_string_lossy().to_string(),
                temp_dir.join("item-5.png").to_string_lossy().to_string(),
            ]
        );

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let paths = thumbnail_schedule_batch(&mut state, tokens.iter(), 64);
        assert_eq!(paths[0], temp_dir.join("item-0.png"));
        assert_eq!(paths[1], temp_dir.join("item-1.png"));
        assert!(
            state
                .panes
                .focused()
                .view
                .has_thumbnail_pending(temp_dir.join("item-0.png").to_str().unwrap())
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn non_candidate_rows_are_marked_once_and_skipped_by_zoom_reschedule() {
        let temp_dir = temp_test_dir("non-candidate-token");
        let config_path = temp_dir.join("sysctl.conf");
        std::fs::write(&config_path, b"kernel.pid_max = 4194304").unwrap();

        let state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let pane_id = state.panes.focused().id;
        let mut entries = vec![test_entry("sysctl.conf", config_path.to_str().unwrap())];

        let media_entries =
            decorate_entries_with_cached_thumbnails_for_pane(&state, pane_id, &mut entries, 64);

        assert!(media_entries.is_empty());
        assert_eq!(entries[0].thumbnail_state, THUMBNAIL_STATE_NOT_CANDIDATE);

        let token = ItemViewRowToken::from_entry(&entries[0]);
        let schedule_entry = ThumbnailScheduleEntry::from_row_token(&token);
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        assert!(
            thumbnail_schedule_batch(&mut state, std::iter::once(&schedule_entry), 64).is_empty()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn thumbnail_schedule_skips_non_candidate_token_without_rechecking_path_type() {
        let temp_dir = temp_test_dir("forced-non-candidate-token");
        let image_path = temp_dir.join("photo.png");
        std::fs::write(&image_path, b"not a real image").unwrap();

        let state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let mut entry = test_entry("photo.png", image_path.to_str().unwrap());
        entry.thumbnail_state = THUMBNAIL_STATE_NOT_CANDIDATE;

        assert!(
            thumbnails::is_thumbnail_candidate(&image_path),
            "fixture should otherwise be eligible for thumbnail scheduling"
        );
        assert!(thumbnail_schedule_candidate(&state, &entry, 64).is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn thumbnail_schedule_batch_marks_pending_in_input_order_and_respects_cap() {
        let temp_dir = temp_test_dir("batch-cap");
        let mut entries = Vec::new();
        for index in 0..(MAX_THUMBNAIL_JOBS_PER_VIEW_SYNC + 8) {
            let path = temp_dir.join(format!("item-{index}.png"));
            std::fs::write(&path, b"not a real image").unwrap();
            entries.push(test_entry(
                &format!("item-{index}.png"),
                path.to_str().unwrap(),
            ));
        }

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let paths = thumbnail_schedule_batch(&mut state, entries.iter(), 64);

        assert_eq!(paths.len(), MAX_THUMBNAIL_JOBS_PER_VIEW_SYNC);
        assert_eq!(paths[0], temp_dir.join("item-0.png"));
        assert_eq!(paths[1], temp_dir.join("item-1.png"));
        assert_eq!(paths[2], temp_dir.join("item-2.png"));
        assert_eq!(paths[3], temp_dir.join("item-3.png"));
        assert!(
            state
                .panes
                .focused()
                .view
                .has_thumbnail_pending(temp_dir.join("item-0.png").to_str().unwrap())
        );
        assert!(
            state
                .panes
                .focused()
                .view
                .has_thumbnail_pending(temp_dir.join("item-0.png").to_str().unwrap())
        );
        assert!(
            !state.panes.focused().view.has_thumbnail_pending(
                temp_dir
                    .join(format!("item-{}.png", MAX_THUMBNAIL_JOBS_PER_VIEW_SYNC + 7))
                    .to_str()
                    .unwrap()
            )
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn thumbnail_schedule_batch_skips_cached_failed_pending_and_directories() {
        let temp_dir = temp_test_dir("batch-skip");
        let cached_path = temp_dir.join("cached.png");
        let failed_path = temp_dir.join("failed.png");
        let pending_path = temp_dir.join("pending.png");
        let dir_path = temp_dir.join("folder.png");
        let ready_path = temp_dir.join("ready.png");
        for path in [&cached_path, &failed_path, &pending_path, &ready_path] {
            std::fs::write(path, b"not a real image").unwrap();
        }
        std::fs::create_dir_all(&dir_path).unwrap();

        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let cached_key = thumbnails::key_for(&cached_path, 64).unwrap();
        let failed_key = thumbnails::key_for(&failed_path, 64).unwrap();
        let pending_key = thumbnails::key_for(&pending_path, 64).unwrap();
        insert_thumbnail_cache_with_limit(
            &mut state,
            cached_key,
            thumbnails::ThumbnailData {
                width: 1,
                height: 1,
                rgba: vec![0, 0, 0, 0],
            },
        );
        insert_thumbnail_failure_with_limit(&mut state, failed_key, "decode failed".to_string());
        state
            .panes
            .focused_mut()
            .view
            .insert_thumbnail_pending(pending_path.to_string_lossy().to_string(), pending_key);

        let mut dir_entry = test_entry("folder.png", dir_path.to_str().unwrap());
        dir_entry.is_dir = true;
        let entries = [
            test_entry("cached.png", cached_path.to_str().unwrap()),
            test_entry("failed.png", failed_path.to_str().unwrap()),
            test_entry("pending.png", pending_path.to_str().unwrap()),
            dir_entry,
            test_entry("ready.png", ready_path.to_str().unwrap()),
        ];
        let paths = thumbnail_schedule_batch(&mut state, entries.iter(), 64);

        assert_eq!(paths, vec![ready_path.clone()]);
        assert!(
            state
                .panes
                .focused()
                .view
                .has_thumbnail_pending(ready_path.to_str().unwrap())
        );
        assert!(
            !state
                .panes
                .focused()
                .view
                .has_thumbnail_pending(cached_path.to_str().unwrap())
        );
        assert!(
            !state
                .panes
                .focused()
                .view
                .has_thumbnail_pending(failed_path.to_str().unwrap())
        );
        assert!(
            !state
                .panes
                .focused()
                .view
                .has_thumbnail_pending(dir_path.to_str().unwrap())
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn thumbnail_schedule_retries_loaded_entry_when_zoom_size_changes() {
        let temp_dir = temp_test_dir("zoom-size-token");
        let image_path = temp_dir.join("photo.png");
        std::fs::write(&image_path, b"not a real image").unwrap();

        let state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let old_key = thumbnails::key_for(&image_path, 64).unwrap();
        let mut entry = test_entry("photo.png", image_path.to_str().unwrap());
        entry.thumbnail_state = THUMBNAIL_STATE_LOADED;
        entry.media_token = old_key.item_view_media_token();

        assert!(
            thumbnail_schedule_candidate(&state, &entry, 64).is_none(),
            "the exact already-loaded thumbnail size should not be rescheduled"
        );
        assert!(
            thumbnail_schedule_candidate(&state, &entry, 128).is_some(),
            "a loaded thumbnail from the previous zoom size must not block the new size"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn virtual_range_path_lookup_uses_identity_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(
            (0..6)
                .map(|index| business_entry(&format!("{index}.png"), &format!("/tmp/{index}.png")))
                .collect(),
        );
        state.panes.focused_mut().view.virtual_view.range = 2..5;

        assert!(path_is_in_virtual_range(&state, "/tmp/2.png"));
        assert!(path_is_in_virtual_range(&state, "/tmp/4.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/1.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/5.png"));
    }

    #[test]
    fn virtual_range_path_lookup_uses_filtered_visible_indices() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.focused_mut().set_file_entries(vec![
            business_entry("alpha.png", "/tmp/alpha.png"),
            business_entry("skip.log", "/tmp/skip.log"),
            business_entry("beta.png", "/tmp/beta.png"),
            business_entry("gamma.png", "/tmp/gamma.png"),
        ]);
        state.panes.focused_mut().search.visible_entry_indices = Some(Arc::from([0, 2, 3]));
        state.panes.focused_mut().view.virtual_view.range = 1..3;

        assert!(path_is_in_virtual_range(&state, "/tmp/beta.png"));
        assert!(path_is_in_virtual_range(&state, "/tmp/gamma.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));
        assert!(!path_is_in_virtual_range(&state, "/tmp/skip.log"));
    }

    #[test]
    fn virtual_range_path_lookup_rejects_empty_or_stale_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state
            .panes
            .focused_mut()
            .set_file_entries(vec![business_entry("alpha.png", "/tmp/alpha.png")]);
        state.panes.focused_mut().view.virtual_view.range = 0..0;
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));

        state.panes.focused_mut().view.virtual_view.range = 9..12;
        assert!(!path_is_in_virtual_range(&state, "/tmp/alpha.png"));
    }
}
