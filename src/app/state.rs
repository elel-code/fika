use crate::fs::privilege::{ExternalEditSession, PrivilegedCommand};
use crate::fs::search;
use crate::fs::thumbnails;
use crate::support::generation::GenerationCounter;
use crate::{DesktopApp, FileEntry, PlaceEntry};
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) current_dir: PathBuf,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) visible_entry_indices: Option<Vec<usize>>,
    pub(crate) virtual_view: VirtualViewCache,
    pub(crate) places: Vec<PlaceEntry>,
    pub(crate) other_application_apps: Vec<DesktopApp>,
    pub(crate) search_query: String,
    pub(crate) search_kind_filter: i32,
    pub(crate) search_modified_filter: i32,
    pub(crate) search_size_filter: i32,
    pub(crate) selected_paths: Vec<String>,
    pub(crate) selection_anchor: Option<String>,
    pub(crate) clipboard_paths: Vec<PathBuf>,
    pub(crate) clipboard_cut: bool,
    pub(crate) chooser_filters: Vec<ChooserFilter>,
    pub(crate) chooser_filter_index: usize,
    pub(crate) chooser_return_filter: bool,
    pub(crate) chooser_choices: Vec<ChooserChoice>,
    pub(crate) chooser_return_choices: bool,
    pub(crate) chooser_parent_window: Option<String>,
    pub(crate) directory_cache: HashMap<PathBuf, Vec<FileEntry>>,
    pub(crate) view_state_cache: HashMap<PathBuf, DirectoryViewState>,
    pub(crate) thumbnail_cache: HashMap<thumbnails::ThumbnailKey, thumbnails::ThumbnailData>,
    pub(crate) thumbnail_cache_order: VecDeque<thumbnails::ThumbnailKey>,
    pub(crate) thumbnail_failures: HashMap<thumbnails::ThumbnailKey, String>,
    pub(crate) thumbnail_failure_order: VecDeque<thumbnails::ThumbnailKey>,
    pub(crate) thumbnail_pending: HashMap<String, thumbnails::ThumbnailKey>,
    pub(crate) operation_queue: VecDeque<FileOperationRequest>,
    pub(crate) active_operation: Option<u64>,
    pub(crate) active_operation_cancel: Option<Arc<AtomicBool>>,
    pub(crate) active_search_cancel: Option<Arc<AtomicBool>>,
    pub(crate) search_progress: search::SearchProgress,
    pub(crate) pending_transfer_conflict: Option<TransferConflict>,
    pub(crate) last_undo: Option<FileUndo>,
    pub(crate) pending_privileged_command: Option<PrivilegedCommand>,
    pub(crate) external_edits: Vec<ExternalEditSession>,
    pub(crate) launched_units: Vec<String>,
    pub(crate) next_operation_id: u64,
    pub(crate) back_stack: Vec<PathBuf>,
    pub(crate) forward_stack: Vec<PathBuf>,
    pub(crate) load_generation: GenerationCounter,
    pub(crate) open_generation: GenerationCounter,
    pub(crate) search_generation: GenerationCounter,
    pub(crate) thumbnail_generation: GenerationCounter,
}

impl AppState {
    pub(crate) fn new(current_dir: PathBuf, places: Vec<PlaceEntry>) -> Self {
        Self {
            current_dir,
            entries: Vec::new(),
            visible_entry_indices: None,
            virtual_view: VirtualViewCache::default(),
            places,
            other_application_apps: Vec::new(),
            search_query: String::new(),
            search_kind_filter: 0,
            search_modified_filter: 0,
            search_size_filter: 0,
            selected_paths: Vec::new(),
            selection_anchor: None,
            clipboard_paths: Vec::new(),
            clipboard_cut: false,
            chooser_filters: Vec::new(),
            chooser_filter_index: 0,
            chooser_return_filter: false,
            chooser_choices: Vec::new(),
            chooser_return_choices: false,
            chooser_parent_window: None,
            directory_cache: HashMap::new(),
            view_state_cache: HashMap::new(),
            thumbnail_cache: HashMap::new(),
            thumbnail_cache_order: VecDeque::new(),
            thumbnail_failures: HashMap::new(),
            thumbnail_failure_order: VecDeque::new(),
            thumbnail_pending: HashMap::new(),
            operation_queue: VecDeque::new(),
            active_operation: None,
            active_operation_cancel: None,
            active_search_cancel: None,
            search_progress: search::SearchProgress::default(),
            pending_transfer_conflict: None,
            last_undo: None,
            pending_privileged_command: None,
            external_edits: Vec::new(),
            launched_units: Vec::new(),
            next_operation_id: 1,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            load_generation: GenerationCounter::default(),
            open_generation: GenerationCounter::default(),
            search_generation: GenerationCounter::default(),
            thumbnail_generation: GenerationCounter::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualViewCache {
    pub(crate) range: Range<usize>,
    pub(crate) entry_count: usize,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) thumbnail_size_px: u32,
}

impl Default for VirtualViewCache {
    fn default() -> Self {
        Self {
            range: 0..0,
            entry_count: 0,
            rows_per_column: 0,
            cell_width: 0.0,
            thumbnail_size_px: 0,
        }
    }
}

impl VirtualViewCache {
    pub(crate) fn invalidate(&mut self) {
        self.range = 0..0;
    }
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DirectoryViewState {
    pub(crate) viewport_x: f32,
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
