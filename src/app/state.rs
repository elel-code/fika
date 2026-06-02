use crate::app::pane::PanesState;
use crate::desktop::clipboard::ClipboardContentKind;
use crate::fs::privilege::{ExternalEditSession, PrivilegedCommand};
use crate::fs::thumbnails;
use crate::support::generation::GenerationCounter;
use crate::{DesktopApp, DeviceEntry, FileEntry, PlaceEntry};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub(crate) const MAX_DIRECTORY_CACHE_ENTRIES: usize = 64;
#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) panes: PanesState,
    pub(crate) places: Vec<PlaceEntry>,
    pub(crate) other_application_apps: Vec<DesktopApp>,
    pub(crate) clipboard_paths: Vec<PathBuf>,
    pub(crate) clipboard_cut: bool,
    pub(crate) clipboard_content_kind: Option<ClipboardContentKind>,
    pub(crate) chooser_filters: Vec<ChooserFilter>,
    pub(crate) chooser_filter_index: usize,
    pub(crate) chooser_return_filter: bool,
    pub(crate) chooser_choices: Vec<ChooserChoice>,
    pub(crate) chooser_return_choices: bool,
    pub(crate) chooser_parent_window: Option<String>,
    pub(crate) directory_cache: HashMap<PathBuf, Vec<FileEntry>>,
    pub(crate) directory_cache_order: VecDeque<PathBuf>,
    pub(crate) directory_prefetch_pending: HashSet<PathBuf>,
    pub(crate) thumbnail_cache: HashMap<thumbnails::ThumbnailKey, thumbnails::ThumbnailData>,
    pub(crate) thumbnail_cache_order: VecDeque<thumbnails::ThumbnailKey>,
    pub(crate) thumbnail_failures: HashMap<thumbnails::ThumbnailKey, String>,
    pub(crate) thumbnail_failure_order: VecDeque<thumbnails::ThumbnailKey>,
    pub(crate) operation_queue: VecDeque<FileOperationRequest>,
    pub(crate) active_operation: Option<u64>,
    pub(crate) active_operation_cancel: Option<Arc<AtomicBool>>,
    pub(crate) active_operation_progress_key: Option<(u64, u64)>,
    pub(crate) pending_transfer_conflict: Option<TransferConflict>,
    pub(crate) last_undo: Option<FileUndo>,
    pub(crate) pending_privileged_command: Option<PrivilegedCommand>,
    pub(crate) external_edits: Vec<ExternalEditSession>,
    pub(crate) devices: Vec<DeviceEntry>,
    pub(crate) pending_device_actions: Vec<DeviceAction>,
    pub(crate) device_errors: HashMap<String, String>,
    pub(crate) launched_units: Vec<String>,
    pub(crate) next_operation_id: u64,
    pub(crate) clipboard_generation: GenerationCounter,
    pub(crate) device_generation: GenerationCounter,
}

impl AppState {
    pub(crate) fn new(current_dir: PathBuf, places: Vec<PlaceEntry>) -> Self {
        Self {
            panes: PanesState::new(current_dir),
            places,
            other_application_apps: Vec::new(),
            clipboard_paths: Vec::new(),
            clipboard_cut: false,
            clipboard_content_kind: None,
            chooser_filters: Vec::new(),
            chooser_filter_index: 0,
            chooser_return_filter: false,
            chooser_choices: Vec::new(),
            chooser_return_choices: false,
            chooser_parent_window: None,
            directory_cache: HashMap::new(),
            directory_cache_order: VecDeque::new(),
            directory_prefetch_pending: HashSet::new(),
            thumbnail_cache: HashMap::new(),
            thumbnail_cache_order: VecDeque::new(),
            thumbnail_failures: HashMap::new(),
            thumbnail_failure_order: VecDeque::new(),
            operation_queue: VecDeque::new(),
            active_operation: None,
            active_operation_cancel: None,
            active_operation_progress_key: None,
            pending_transfer_conflict: None,
            last_undo: None,
            pending_privileged_command: None,
            external_edits: Vec::new(),
            devices: Vec::new(),
            pending_device_actions: Vec::new(),
            device_errors: HashMap::new(),
            launched_units: Vec::new(),
            next_operation_id: 1,
            clipboard_generation: GenerationCounter::default(),
            device_generation: GenerationCounter::default(),
        }
    }

    pub(crate) fn cached_directory_entries(&mut self, path: &Path) -> Option<Vec<FileEntry>> {
        let entries = self.directory_cache.get(path).cloned()?;
        self.refresh_directory_cache_order(path);
        Some(entries)
    }

    pub(crate) fn insert_directory_cache(&mut self, path: PathBuf, entries: Vec<FileEntry>) {
        self.directory_cache.insert(path.clone(), entries);
        self.refresh_directory_cache_order(&path);
        while self.directory_cache_order.len() > MAX_DIRECTORY_CACHE_ENTRIES {
            if let Some(oldest) = self.directory_cache_order.pop_front() {
                self.directory_cache.remove(&oldest);
            }
        }
    }

    pub(crate) fn remove_directory_cache(&mut self, path: &Path) {
        self.directory_cache.remove(path);
        self.directory_cache_order
            .retain(|cached| cached.as_path() != path);
    }

    fn refresh_directory_cache_order(&mut self, path: &Path) {
        self.directory_cache_order
            .retain(|cached| cached.as_path() != path);
        self.directory_cache_order.push_back(path.to_path_buf());
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeviceAction {
    pub(crate) device_path: String,
    pub(crate) action: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChooserFilter {
    pub(crate) label: String,
    pub(crate) patterns: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChooserChoiceItem {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChooserChoice {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) items: Vec<ChooserChoiceItem>,
    pub(crate) selected_index: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct FileOperationRequest {
    pub(crate) id: u64,
    pub(crate) operation: String,
    pub(crate) source: PathBuf,
    pub(crate) target_dir: PathBuf,
    pub(crate) conflict_policy: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TransferConflict {
    pub(crate) operation: String,
    pub(crate) source: PathBuf,
    pub(crate) target_dir: PathBuf,
    pub(crate) destination: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct FileUndo {
    pub(crate) operation: String,
    pub(crate) original_source: PathBuf,
    pub(crate) destination: PathBuf,
    pub(crate) overwritten_backup: Option<PathBuf>,
    pub(crate) items: Vec<FileUndoItem>,
}

#[derive(Clone, Debug)]
pub(crate) struct FileUndoItem {
    pub(crate) original_source: PathBuf,
    pub(crate) destination: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Image;

    #[test]
    fn directory_cache_evicts_oldest_entries() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        for index in 0..(MAX_DIRECTORY_CACHE_ENTRIES + 3) {
            let path = PathBuf::from(format!("/tmp/dir-{index}"));
            state.insert_directory_cache(path.clone(), vec![test_entry(index)]);
        }

        assert_eq!(state.directory_cache.len(), MAX_DIRECTORY_CACHE_ENTRIES);
        assert_eq!(
            state.directory_cache_order.len(),
            MAX_DIRECTORY_CACHE_ENTRIES
        );
        assert!(!state.directory_cache.contains_key(Path::new("/tmp/dir-0")));
        assert!(!state.directory_cache.contains_key(Path::new("/tmp/dir-2")));
        assert!(state.directory_cache.contains_key(Path::new("/tmp/dir-3")));
    }

    #[test]
    fn directory_cache_hit_refreshes_lru_order() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let first = PathBuf::from("/tmp/first");
        let second = PathBuf::from("/tmp/second");

        state.insert_directory_cache(first.clone(), vec![test_entry(1)]);
        state.insert_directory_cache(second.clone(), vec![test_entry(2)]);

        assert_eq!(
            state.cached_directory_entries(&first).map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| entry.path.to_string())
                    .collect::<Vec<_>>()
            }),
            Some(vec!["/tmp/file-1".to_string()])
        );
        assert_eq!(state.directory_cache_order.pop_back(), Some(first));
        assert_eq!(state.directory_cache_order.pop_front(), Some(second));
    }

    #[test]
    fn directory_cache_remove_clears_lru_order() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        let path = PathBuf::from("/tmp/remove-me");

        state.insert_directory_cache(path.clone(), vec![test_entry(1)]);
        state.remove_directory_cache(&path);

        assert!(!state.directory_cache.contains_key(&path));
        assert!(!state.directory_cache_order.contains(&path));
    }

    fn test_entry(index: usize) -> FileEntry {
        FileEntry {
            name: format!("file-{index}").into(),
            path: format!("/tmp/file-{index}").into(),
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
