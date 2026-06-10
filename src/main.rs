mod ui;

use fika_core::{
    CompactColumnMetrics, CompactLayout, CompactLayoutOptions, CreateUndoItem, CreatedItemKind,
    DirectoryCache, DirectoryLister, DirectoryListerEvent, FileClipboardRole, OperationQueue,
    PaneController, PaneId, RenameUndoItem, SMOOTH_SCROLL_FRAME, ScrollBounds, ScrollDragTracker,
    SelectionMove, SmoothScroll, SortDescriptor, SortOrder, SortRole, TransferUndoItem,
    TrashUndoItem, UndoPayload, UndoRecord, UserPlace, ViewPoint, ViewRect, ViewState, ZoomChange,
    decode_file_clipboard_text, encode_file_clipboard_text, file_ops, nearest_existing_ancestor,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardEntry, ClipboardItem, Context, Div, IntoElement, MouseButton,
    ParentElement, Render, Stateful, Styled, Window, WindowBounds, WindowOptions, div, px, rgb,
    rgba, size,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque, hash_map::DefaultHasher};
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Manager,
    Chooser,
}

#[derive(Clone, Debug)]
struct Args {
    mode: Mode,
    start_dir: PathBuf,
    chooser_directories: bool,
    chooser_multiple: bool,
    chooser_title: Option<String>,
    chooser_accept_label: Option<String>,
    chooser_filter_index: usize,
    chooser_return_filter: bool,
    chooser_choices: Vec<String>,
    chooser_return_choices: bool,
}

impl Args {
    fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut mode = Mode::Manager;
        let mut start_dir = None;
        let mut chooser_directories = false;
        let mut chooser_multiple = false;
        let mut chooser_title = None;
        let mut chooser_accept_label = None;
        let mut chooser_filter_index = 0usize;
        let mut chooser_return_filter = false;
        let mut chooser_choices = Vec::new();
        let mut chooser_return_choices = false;
        let mut pending_title = false;
        let mut pending_accept_label = false;
        let mut pending_filter_index = false;
        let mut pending_choices = false;
        let mut skip_next = false;

        for arg in args {
            if skip_next {
                skip_next = false;
                continue;
            }
            if pending_title {
                chooser_title = (!arg.is_empty()).then_some(arg);
                pending_title = false;
                continue;
            }
            if pending_accept_label {
                chooser_accept_label = (!arg.is_empty()).then_some(arg);
                pending_accept_label = false;
                continue;
            }
            if pending_filter_index {
                chooser_filter_index = arg.parse().unwrap_or_default();
                pending_filter_index = false;
                continue;
            }
            if pending_choices {
                chooser_choices = arg
                    .split('\n')
                    .filter(|choice| !choice.is_empty())
                    .map(str::to_string)
                    .collect();
                pending_choices = false;
                continue;
            }

            match arg.as_str() {
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "--chooser" => mode = Mode::Chooser,
                "--chooser-directory" => {
                    mode = Mode::Chooser;
                    chooser_directories = true;
                }
                "--chooser-multiple" => {
                    mode = Mode::Chooser;
                    chooser_multiple = true;
                }
                "--chooser-save"
                | "--chooser-save-files"
                | "--chooser-filters"
                | "--chooser-parent-window" => {
                    mode = Mode::Chooser;
                    skip_next = true;
                }
                "--chooser-title" => {
                    mode = Mode::Chooser;
                    pending_title = true;
                }
                "--chooser-accept-label" => {
                    mode = Mode::Chooser;
                    pending_accept_label = true;
                }
                "--chooser-filter-index" => {
                    mode = Mode::Chooser;
                    pending_filter_index = true;
                }
                "--chooser-return-filter" => {
                    mode = Mode::Chooser;
                    chooser_return_filter = true;
                }
                "--chooser-choices" => {
                    mode = Mode::Chooser;
                    pending_choices = true;
                }
                "--chooser-return-choices" => {
                    mode = Mode::Chooser;
                    chooser_return_choices = true;
                }
                _ if start_dir.is_none() => start_dir = Some(expand_user_path(&arg)),
                _ => {}
            }
        }

        let start_dir = normalize_start_dir(start_dir.unwrap_or_else(home_dir));
        Self {
            mode,
            start_dir,
            chooser_directories,
            chooser_multiple,
            chooser_title,
            chooser_accept_label,
            chooser_filter_index,
            chooser_return_filter,
            chooser_choices,
            chooser_return_choices,
        }
    }
}

#[derive(Clone, Debug)]
struct ChooserState {
    directories: bool,
    multiple: bool,
    title: String,
    accept_label: String,
    filter_index: usize,
    return_filter: bool,
    choices: Vec<String>,
    return_choices: bool,
}

#[derive(Clone, Debug)]
struct PaneSnapshot {
    id: PaneId,
    breadcrumbs: Vec<BreadcrumbSegment>,
    location_draft: Option<String>,
    filter_bar: Option<FilterBarSnapshot>,
    status_bar: StatusBarSnapshot,
    layout: CompactLayout,
    visible_items: Vec<VisibleItemSnapshot>,
    view: ViewState,
    rubber_band: Option<ViewRect>,
    focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FilterBarSnapshot {
    pub(crate) query: String,
    pub(crate) focused: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) mode: fika_core::NameFilterMode,
    pub(crate) match_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct StatusBarSnapshot {
    pub(crate) message: String,
    pub(crate) item_summary: String,
    pub(crate) free_space: Option<SpaceInfoSnapshot>,
    pub(crate) zoom_level: i32,
    pub(crate) zoom_icon_size: f32,
    pub(crate) zoom_min: i32,
    pub(crate) zoom_max: i32,
    pub(crate) operation_pending: bool,
    pub(crate) operation_progress: Option<OperationProgressSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SpaceInfoSnapshot {
    pub(crate) free_label: String,
    pub(crate) detail_label: String,
    pub(crate) used_percent: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OperationProgressSnapshot {
    pub(crate) label: String,
    pub(crate) bytes_done: u64,
    pub(crate) bytes_total: u64,
    pub(crate) percent: Option<u8>,
    pub(crate) cancellable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BreadcrumbSegment {
    pub(crate) label: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct VisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) layout: fika_core::ItemLayout,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) kind_label: String,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) selected: bool,
    pub(crate) draft_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileIconSnapshot {
    pub(crate) marker: String,
    pub(crate) fg: u32,
    pub(crate) bg: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum FileIconKey {
    Directory,
    Extension(String),
    File,
}

#[derive(Clone, Debug, Default)]
struct FileIconCache {
    cached: HashMap<FileIconKey, FileIconSnapshot>,
}

impl FileIconCache {
    fn icon_for(&mut self, path: &Path, is_dir: bool) -> FileIconSnapshot {
        let key = file_icon_key(path, is_dir);
        if let Some(icon) = self.cached.get(&key) {
            return icon.clone();
        }

        let icon = file_icon_snapshot(&key);
        self.cached.insert(key, icon.clone());
        icon
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ContextMenuState {
    pane_id: PaneId,
    target: ContextMenuTarget,
    position: ViewPoint,
    active_submenu: Option<ContextMenuOpenSubmenu>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContextMenuOpenSubmenu {
    submenu: ContextMenuSubmenu,
    parent_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
enum ContextMenuTarget {
    Blank {
        trash_view: bool,
        trash_has_items: bool,
    },
    PlacesBlank {
        hidden_sections: bool,
    },
    PlaceSection {
        group: &'static str,
    },
    Place {
        path: PathBuf,
        trash_place: bool,
        trash_has_items: bool,
        editable: bool,
        removable: bool,
    },
    Item {
        path: PathBuf,
        is_dir: bool,
        selection_count: usize,
        trash_view: bool,
        trash_can_restore: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMenuAction {
    Open,
    OpenInNewPane,
    AddPlace,
    EditPlace,
    RemovePlace,
    HidePlaceSection,
    ShowHiddenPlaces,
    SortBySubmenu,
    ViewModeSubmenu,
    SortByName,
    SortByModified,
    SortBySize,
    SortAscending,
    SortDescending,
    SortFoldersFirst,
    SortHiddenLast,
    ViewCompact,
    ViewIcons,
    ViewDetails,
    Rename,
    Copy,
    CopyLocation,
    Cut,
    Trash,
    RestoreFromTrash,
    DeletePermanently,
    EmptyTrash,
    Properties,
    CreateFolder,
    Paste,
    SelectAll,
    Refresh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContextMenuItem {
    action: ContextMenuAction,
    label: &'static str,
    enabled: bool,
    submenu: Option<ContextMenuSubmenu>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMenuSubmenu {
    SortBy,
    ViewMode,
}

const CONTEXT_MENU_WIDTH: f32 = 196.0;
const CONTEXT_MENU_ROW_HEIGHT: f32 = 28.0;
const CONTEXT_MENU_VERTICAL_PADDING: f32 = 4.0;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PropertiesDialogState {
    title: String,
    rows: Vec<PropertyRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PropertyRow {
    label: &'static str,
    value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ContentItemHit {
    model_index: usize,
    path: PathBuf,
    is_dir: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PlaceSnapshot {
    pub(crate) group: &'static str,
    pub(crate) marker: &'static str,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) active: bool,
    pub(crate) trash_place: bool,
    pub(crate) trash_has_items: bool,
    pub(crate) editable: bool,
    pub(crate) removable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlaceEntry {
    group: &'static str,
    marker: &'static str,
    label: String,
    path: PathBuf,
    editable: bool,
    removable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlaceDraft {
    pane_id: PaneId,
    editing_path: Option<PathBuf>,
    focus: PlaceDraftField,
    label: String,
    path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaceDraftField {
    Label,
    Path,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RubberBandState {
    pane_id: PaneId,
    start: ViewPoint,
    current: ViewPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RubberBandDrag {
    pane_id: PaneId,
}

impl RubberBandState {
    fn rect(self) -> ViewRect {
        let x = self.start.x.min(self.current.x);
        let y = self.start.y.min(self.current.y);
        ViewRect {
            x,
            y,
            width: self.start.x.max(self.current.x) - x,
            height: self.start.y.max(self.current.y) - y,
        }
    }

    fn visible_rect(self, scroll_x: f32, scroll_y: f32) -> ViewRect {
        let rect = self.rect();
        ViewRect {
            x: rect.x - scroll_x,
            y: rect.y - scroll_y,
            width: rect.width,
            height: rect.height,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenameDraft {
    pane_id: PaneId,
    original_path: PathBuf,
    draft_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocationDraft {
    pane_id: PaneId,
    value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PaneFilterState {
    visible: bool,
    focused: bool,
    query: String,
    mode: fika_core::NameFilterMode,
    case_sensitive: bool,
}

impl Default for PaneFilterState {
    fn default() -> Self {
        Self {
            visible: false,
            focused: false,
            query: String::new(),
            mode: fika_core::NameFilterMode::Glob,
            case_sensitive: false,
        }
    }
}

impl PaneFilterState {
    fn active_filter(&self) -> Option<fika_core::NameFilter> {
        if self.query.is_empty() {
            return None;
        }
        let filter = match self.mode {
            fika_core::NameFilterMode::PlainText => {
                fika_core::NameFilter::plain_text(self.query.clone())
            }
            fika_core::NameFilterMode::Glob => fika_core::NameFilter::glob(self.query.clone()),
        }
        .with_case_sensitive(self.case_sensitive);
        Some(filter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FilteredModelCacheKey {
    model_generation: u64,
    filter: fika_core::NameFilter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FilteredModelCacheEntry {
    key: FilteredModelCacheKey,
    model: fika_core::FilteredModel,
}

#[derive(Clone, Debug)]
struct PaneLayoutProjection {
    layout: CompactLayout,
    filtered: Option<fika_core::FilteredModel>,
}

impl PaneLayoutProjection {
    fn model_index_for_layout_index(&self, layout_index: usize) -> Option<usize> {
        model_index_for_layout_index(self.filtered.as_ref(), layout_index)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClipboardMode {
    Copy,
    Cut,
}

impl ClipboardMode {
    fn operation(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Cut => "move",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Cut => "Move",
        }
    }

    fn file_clipboard_role(self) -> FileClipboardRole {
        match self {
            Self::Copy => FileClipboardRole::Copy,
            Self::Cut => FileClipboardRole::Cut,
        }
    }

    fn from_file_clipboard_role(role: FileClipboardRole) -> Self {
        match role {
            FileClipboardRole::Copy => Self::Copy,
            FileClipboardRole::Cut => Self::Cut,
        }
    }

    fn metadata_tag(self) -> &'static str {
        match self {
            Self::Copy => "fika-file-clipboard:copy",
            Self::Cut => "fika-file-clipboard:cut",
        }
    }

    fn from_metadata_tag(tag: &str) -> Option<Self> {
        match tag {
            "fika-file-clipboard:copy" => Some(Self::Copy),
            "fika-file-clipboard:cut" => Some(Self::Cut),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClipboardState {
    mode: ClipboardMode,
    paths: Vec<PathBuf>,
    text: Option<String>,
}

impl ClipboardState {
    fn files(mode: ClipboardMode, paths: Vec<PathBuf>) -> Self {
        Self {
            mode,
            paths,
            text: None,
        }
    }

    fn text(text: String) -> Option<Self> {
        (!text.is_empty()).then_some(Self {
            mode: ClipboardMode::Copy,
            paths: Vec::new(),
            text: Some(text),
        })
    }

    fn to_clipboard_item(&self) -> ClipboardItem {
        if let Some(text) = &self.text {
            return ClipboardItem::new_string(text.clone());
        }
        ClipboardItem::new_string_with_metadata(
            encode_file_clipboard_text(self.mode.file_clipboard_role(), &self.paths),
            self.mode.metadata_tag().to_string(),
        )
    }

    fn from_clipboard_item(item: &ClipboardItem) -> Option<Self> {
        let metadata_mode = item
            .metadata()
            .and_then(|tag| ClipboardMode::from_metadata_tag(tag.as_str()));
        let external_paths = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::ExternalPaths(paths) => Some(paths.paths()),
                _ => None,
            })
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if !external_paths.is_empty() {
            return Some(Self {
                mode: metadata_mode.unwrap_or(ClipboardMode::Copy),
                paths: external_paths,
                text: None,
            });
        }

        let text = item.text()?;
        if let Some(payload) = decode_file_clipboard_text(&text) {
            return Some(Self {
                mode: metadata_mode
                    .unwrap_or_else(|| ClipboardMode::from_file_clipboard_role(payload.role)),
                paths: payload.paths,
                text: None,
            });
        }

        Self::text(text)
    }

    fn item_count(&self) -> usize {
        if self.text.is_some() {
            1
        } else {
            self.paths.len()
        }
    }

    fn action_label(&self) -> &'static str {
        if self.text.is_some() {
            "Paste"
        } else {
            self.mode.label()
        }
    }

    fn progress_label(&self) -> String {
        if self.text.is_some() {
            "Pasting text".to_string()
        } else {
            format!("{}ing {} item(s)", self.mode.label(), self.item_count())
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SpaceInfoCache {
    path: Option<PathBuf>,
    snapshot: Option<SpaceInfoSnapshot>,
    request_in_flight: bool,
    last_requested: Option<Instant>,
}

impl SpaceInfoCache {
    const RETRY_AFTER: Duration = Duration::from_secs(30);

    fn snapshot_for(&self, path: &Path) -> Option<SpaceInfoSnapshot> {
        (self.path.as_deref() == Some(path))
            .then(|| self.snapshot.clone())
            .flatten()
    }

    fn should_request(&self, path: &Path, now: Instant) -> bool {
        if self.request_in_flight && self.path.as_deref() == Some(path) {
            return false;
        }
        if self.path.as_deref() != Some(path) {
            return true;
        }
        if self.snapshot.is_some() {
            return false;
        }
        self.last_requested
            .is_none_or(|last_requested| now.duration_since(last_requested) >= Self::RETRY_AFTER)
    }

    fn start_request(&mut self, path: PathBuf, now: Instant) {
        self.path = Some(path);
        self.snapshot = None;
        self.request_in_flight = true;
        self.last_requested = Some(now);
    }

    fn finish_request(&mut self, path: &Path, snapshot: Option<SpaceInfoSnapshot>) -> bool {
        if self.path.as_deref() != Some(path) {
            return false;
        }
        self.request_in_flight = false;
        self.snapshot = snapshot;
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StatusSummaryCacheKey {
    model_generation: u64,
    model_len: usize,
    filter_revision: u64,
    visible_len: usize,
    selection_count: usize,
    selection_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StatusSummaryCacheEntry {
    key: StatusSummaryCacheKey,
    summary: String,
}

#[derive(Clone, Debug)]
struct OperationProgressHandle {
    pane_id: PaneId,
    label: String,
    progress: Arc<Mutex<file_ops::TransferProgress>>,
    cancel: Option<Arc<AtomicBool>>,
    started_at: Instant,
}

impl OperationProgressHandle {
    fn snapshot(&self, now: Instant) -> Option<OperationProgressSnapshot> {
        if !progress_delay_elapsed(self.started_at, now) {
            return None;
        }
        let progress = *self
            .progress
            .lock()
            .expect("operation progress state poisoned");
        Some(OperationProgressSnapshot {
            label: self.label.clone(),
            bytes_done: progress.bytes_done,
            bytes_total: progress.bytes_total,
            percent: progress_percent(progress.bytes_done, progress.bytes_total),
            cancellable: self.cancel.is_some(),
        })
    }
}

const PROGRESS_DISPLAY_DELAY: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug)]
struct LoadingPaneState {
    key: ListingRequestKey,
    started_at: Instant,
}

fn progress_percent(bytes_done: u64, bytes_total: u64) -> Option<u8> {
    if bytes_total == 0 {
        return None;
    }
    Some(((bytes_done.saturating_mul(100) + (bytes_total / 2)) / bytes_total).min(100) as u8)
}

fn progress_delay_elapsed(started_at: Instant, now: Instant) -> bool {
    now.duration_since(started_at) >= PROGRESS_DISPLAY_DELAY
}

#[derive(Clone, Debug, Default)]
struct VisibleItemSlotPool {
    next_slot_id: u64,
    slot_by_item_id: BTreeMap<fika_core::ItemId, u64>,
    free_slots: Vec<u64>,
}

impl VisibleItemSlotPool {
    const MAX_FREE_SLOTS: usize = 100;

    fn slots_for_items(
        &mut self,
        visible_item_ids: impl IntoIterator<Item = fika_core::ItemId>,
    ) -> BTreeMap<fika_core::ItemId, u64> {
        let visible_item_ids = visible_item_ids.into_iter().collect::<BTreeSet<_>>();
        let previous = std::mem::take(&mut self.slot_by_item_id);
        for (item_id, slot_id) in previous {
            if visible_item_ids.contains(&item_id) {
                self.slot_by_item_id.insert(item_id, slot_id);
            } else {
                self.free_slots.push(slot_id);
            }
        }
        if self.free_slots.len() > Self::MAX_FREE_SLOTS {
            self.free_slots.truncate(Self::MAX_FREE_SLOTS);
        }

        for item_id in visible_item_ids {
            if self.slot_by_item_id.contains_key(&item_id) {
                continue;
            }
            let slot_id = self.free_slots.pop().unwrap_or_else(|| {
                self.next_slot_id += 1;
                self.next_slot_id
            });
            self.slot_by_item_id.insert(item_id, slot_id);
        }

        self.slot_by_item_id.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CompactColumnWidthCacheKey {
    generation: u64,
    source_revision: u64,
    item_count: usize,
    rows_per_column: usize,
    min_item_width: f32,
    icon_size: f32,
    padding: f32,
    gap: f32,
}

#[derive(Clone, Debug, Default)]
struct CompactColumnWidthCache {
    cached: Vec<CompactColumnWidthCacheEntry>,
}

#[derive(Clone, Debug)]
struct CompactColumnWidthCacheEntry {
    key: CompactColumnWidthCacheKey,
    widths: Vec<f32>,
    resolved_columns: Vec<bool>,
    metrics: Option<CompactColumnMetrics>,
}

const AVERAGE_COMPACT_CHAR_WIDTH: f32 = 7.0;

impl CompactColumnWidthCache {
    const MAX_CACHED_LAYOUTS: usize = 4;
    const COLUMN_OVERSCAN: usize = 2;

    fn metrics_for_model(
        &mut self,
        model: &fika_core::DirectoryModel,
        rows_per_column: usize,
        options: CompactLayoutOptions,
    ) -> CompactColumnMetrics {
        self.metrics_for_model_view(model, None, 0, rows_per_column, options)
    }

    fn metrics_for_filtered_model(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: &fika_core::FilteredModel,
        source_revision: u64,
        rows_per_column: usize,
        options: CompactLayoutOptions,
    ) -> CompactColumnMetrics {
        self.metrics_for_model_view(
            model,
            Some(filtered),
            source_revision,
            rows_per_column,
            options,
        )
    }

    fn metrics_for_model_view(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: Option<&fika_core::FilteredModel>,
        source_revision: u64,
        rows_per_column: usize,
        options: CompactLayoutOptions,
    ) -> CompactColumnMetrics {
        let item_count = filtered.map_or_else(|| model.len(), fika_core::FilteredModel::len);
        let key = CompactColumnWidthCacheKey {
            generation: model.data_generation(),
            source_revision,
            item_count,
            rows_per_column,
            min_item_width: options.item_width,
            icon_size: options.icon_size,
            padding: options.padding,
            gap: options.gap,
        };
        let column_count = item_count.div_ceil(rows_per_column);
        let position = self.cached.iter().position(|entry| entry.key == key);
        let position = match position {
            Some(position) => position,
            None => {
                if self.cached.len() >= Self::MAX_CACHED_LAYOUTS {
                    self.cached.remove(0);
                }
                self.cached.push(CompactColumnWidthCacheEntry::new(
                    key,
                    column_count,
                    options,
                ));
                self.cached.len() - 1
            }
        };

        let entry = &mut self.cached[position];
        entry.resolve_visible_columns(model, filtered, item_count, rows_per_column, options);
        entry.metrics(options)
    }
}

impl CompactColumnWidthCacheEntry {
    fn new(
        key: CompactColumnWidthCacheKey,
        column_count: usize,
        options: CompactLayoutOptions,
    ) -> Self {
        Self {
            key,
            widths: vec![options.item_width; column_count],
            resolved_columns: vec![false; column_count],
            metrics: None,
        }
    }

    fn metrics(&mut self, options: CompactLayoutOptions) -> CompactColumnMetrics {
        if let Some(metrics) = &self.metrics {
            return metrics.clone();
        }
        let metrics = CompactColumnMetrics::new(
            self.widths.len(),
            options.item_width,
            options.padding,
            options.gap,
            self.widths.clone(),
        );
        self.metrics = Some(metrics.clone());
        metrics
    }

    fn resolve_visible_columns(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: Option<&fika_core::FilteredModel>,
        item_count: usize,
        rows_per_column: usize,
        options: CompactLayoutOptions,
    ) {
        if self.widths.is_empty() {
            return;
        }

        for _ in 0..2 {
            let metrics = self.metrics(options);
            let layout = CompactLayout::new_with_column_metrics(item_count, options, metrics);
            let range = overscanned_column_range(
                layout.visible_column_range(),
                self.widths.len(),
                CompactColumnWidthCache::COLUMN_OVERSCAN,
            );
            if range.is_empty()
                || !self.resolve_columns(
                    model,
                    filtered,
                    item_count,
                    rows_per_column,
                    options,
                    range,
                )
            {
                break;
            }
        }
    }

    fn resolve_columns(
        &mut self,
        model: &fika_core::DirectoryModel,
        filtered: Option<&fika_core::FilteredModel>,
        item_count: usize,
        rows_per_column: usize,
        options: CompactLayoutOptions,
        columns: std::ops::Range<usize>,
    ) -> bool {
        let mut width_changed = false;
        for column in columns {
            if self
                .resolved_columns
                .get(column)
                .copied()
                .unwrap_or_default()
            {
                continue;
            }
            let start = column * rows_per_column;
            let end = (start + rows_per_column).min(item_count);
            let mut width = options.item_width;
            for layout_index in start..end {
                let Some(model_index) = model_index_for_layout_index(filtered, layout_index) else {
                    continue;
                };
                if let Some(entry) = model.get(model_index) {
                    width = width.max(required_compact_item_width(entry, options));
                }
            }
            if let Some(resolved) = self.resolved_columns.get_mut(column) {
                *resolved = true;
            }
            if let Some(cached_width) = self.widths.get_mut(column)
                && (*cached_width - width).abs() > f32::EPSILON
            {
                *cached_width = width;
                width_changed = true;
            }
        }

        if width_changed {
            self.metrics = None;
        }
        width_changed
    }
}

fn overscanned_column_range(
    range: std::ops::Range<usize>,
    column_count: usize,
    overscan: usize,
) -> std::ops::Range<usize> {
    if column_count == 0 || range.is_empty() {
        return 0..0;
    }
    range.start.saturating_sub(overscan)..(range.end + overscan).min(column_count)
}

fn required_compact_item_width(entry: &fika_core::EntryData, options: CompactLayoutOptions) -> f32 {
    options.padding * 4.0 + options.icon_size + compact_text_width(entry.name_width_units)
}

fn compact_text_width(name_width_units: u16) -> f32 {
    f32::from(name_width_units) * AVERAGE_COMPACT_CHAR_WIDTH
}

fn model_index_for_layout_index(
    filtered: Option<&fika_core::FilteredModel>,
    layout_index: usize,
) -> Option<usize> {
    filtered.map_or(Some(layout_index), |filtered| {
        filtered.model_index(layout_index)
    })
}

fn filter_source_revision(filter: &fika_core::NameFilter) -> u64 {
    let mut hasher = DefaultHasher::new();
    filter.hash(&mut hasher);
    match hasher.finish() {
        0 => 1,
        revision => revision,
    }
}

fn format_entry_kind_label(entry: &fika_core::EntryData) -> String {
    if let Some(deletion_time) = &entry.trash_deletion_time {
        return fika_core::format_trash_deletion_time(deletion_time);
    }
    if entry.is_dir {
        "Folder".to_string()
    } else {
        fika_core::format_size(entry.size_bytes)
    }
}

fn file_icon_key(path: &Path, is_dir: bool) -> FileIconKey {
    if is_dir {
        return FileIconKey::Directory;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| FileIconKey::Extension(extension.to_ascii_lowercase()))
        .unwrap_or(FileIconKey::File)
}

fn file_icon_snapshot(key: &FileIconKey) -> FileIconSnapshot {
    match key {
        FileIconKey::Directory => FileIconSnapshot {
            marker: "DIR".to_string(),
            fg: 0x0f4c81,
            bg: 0xe7f1fb,
        },
        FileIconKey::Extension(extension) if is_image_extension(extension) => FileIconSnapshot {
            marker: "IMG".to_string(),
            fg: 0x7c2d12,
            bg: 0xffedd5,
        },
        FileIconKey::Extension(extension) if is_archive_extension(extension) => FileIconSnapshot {
            marker: "ARC".to_string(),
            fg: 0x713f12,
            bg: 0xfef3c7,
        },
        FileIconKey::Extension(extension) if is_audio_extension(extension) => FileIconSnapshot {
            marker: "AUD".to_string(),
            fg: 0x6d28d9,
            bg: 0xf3e8ff,
        },
        FileIconKey::Extension(extension) if is_video_extension(extension) => FileIconSnapshot {
            marker: "VID".to_string(),
            fg: 0x9f1239,
            bg: 0xffe4e6,
        },
        FileIconKey::Extension(extension) if extension == "rs" => FileIconSnapshot {
            marker: "RS".to_string(),
            fg: 0x7c2d12,
            bg: 0xfff7ed,
        },
        FileIconKey::Extension(extension) if extension == "pdf" => FileIconSnapshot {
            marker: "PDF".to_string(),
            fg: 0x991b1b,
            bg: 0xfee2e2,
        },
        FileIconKey::Extension(extension) if extension == "md" || extension == "txt" => {
            FileIconSnapshot {
                marker: "TXT".to_string(),
                fg: 0x334155,
                bg: 0xf1f5f9,
            }
        }
        FileIconKey::Extension(extension) if extension.len() <= 3 => FileIconSnapshot {
            marker: extension.to_ascii_uppercase(),
            fg: 0x374151,
            bg: 0xf3f4f6,
        },
        FileIconKey::Extension(_) | FileIconKey::File => FileIconSnapshot {
            marker: "FILE".to_string(),
            fg: 0x475569,
            bg: 0xf1f5f9,
        },
    }
}

fn is_image_extension(extension: &str) -> bool {
    matches!(
        extension,
        "avif" | "bmp" | "gif" | "heic" | "jpeg" | "jpg" | "png" | "svg" | "tif" | "tiff" | "webp"
    )
}

fn is_archive_extension(extension: &str) -> bool {
    matches!(
        extension,
        "7z" | "bz2" | "gz" | "rar" | "tar" | "xz" | "zip" | "zst"
    )
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension,
        "aac" | "flac" | "m4a" | "mp3" | "ogg" | "opus" | "wav"
    )
}

fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension,
        "avi" | "m4v" | "mkv" | "mov" | "mp4" | "mpeg" | "mpg" | "webm"
    )
}

fn compact_layout_for_model(
    cache: &mut CompactColumnWidthCache,
    model: &fika_core::DirectoryModel,
    view: &ViewState,
) -> CompactLayout {
    compact_layout_for_model_view(cache, model, None, 0, view)
}

fn compact_layout_for_filtered_model(
    cache: &mut CompactColumnWidthCache,
    model: &fika_core::DirectoryModel,
    filtered: &fika_core::FilteredModel,
    source_revision: u64,
    view: &ViewState,
) -> CompactLayout {
    compact_layout_for_model_view(cache, model, Some(filtered), source_revision, view)
}

fn compact_layout_for_model_view(
    cache: &mut CompactColumnWidthCache,
    model: &fika_core::DirectoryModel,
    filtered: Option<&fika_core::FilteredModel>,
    source_revision: u64,
    view: &ViewState,
) -> CompactLayout {
    let item_count = filtered.map_or_else(|| model.len(), fika_core::FilteredModel::len);
    let options = ui::file_grid::compact_layout_options(view, 0.0);
    let rows_per_column = CompactLayout::rows_per_column_for_options(options);
    let metrics = match filtered {
        Some(filtered) => cache.metrics_for_filtered_model(
            model,
            filtered,
            source_revision,
            rows_per_column,
            options,
        ),
        None => cache.metrics_for_model(model, rows_per_column, options),
    };
    let layout = CompactLayout::new_with_column_metrics(item_count, options, metrics);

    if layout
        .horizontal_scroll_bar(
            ui::file_grid::SCROLLBAR_THICKNESS,
            ui::file_grid::SCROLLBAR_MIN_HANDLE_WIDTH,
        )
        .is_none()
    {
        return layout;
    }

    let options = ui::file_grid::compact_layout_options(view, ui::file_grid::SCROLLBAR_THICKNESS);
    let rows_per_column = CompactLayout::rows_per_column_for_options(options);
    let metrics = match filtered {
        Some(filtered) => cache.metrics_for_filtered_model(
            model,
            filtered,
            source_revision,
            rows_per_column,
            options,
        ),
        None => cache.metrics_for_model(model, rows_per_column, options),
    };
    CompactLayout::new_with_column_metrics(item_count, options, metrics)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ListingRequestKey {
    generation: fika_core::Generation,
    request_serial: fika_core::RequestSerial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ListingRequest {
    pane_id: PaneId,
    generation: fika_core::Generation,
    request_serial: fika_core::RequestSerial,
    path: PathBuf,
    mode: fika_core::LoadMode,
}

impl ListingRequest {
    fn from_event(event: &DirectoryListerEvent) -> Option<Self> {
        let DirectoryListerEvent::LoadingStarted {
            pane_id,
            generation,
            request_serial,
            path,
            mode,
        } = event
        else {
            return None;
        };
        Some(Self {
            pane_id: *pane_id,
            generation: *generation,
            request_serial: *request_serial,
            path: path.clone(),
            mode: *mode,
        })
    }

    fn key(&self) -> ListingRequestKey {
        ListingRequestKey {
            generation: self.generation,
            request_serial: self.request_serial,
        }
    }
}

fn listing_requests_from_events<'a>(
    events: impl IntoIterator<Item = &'a DirectoryListerEvent>,
) -> Vec<ListingRequest> {
    events
        .into_iter()
        .filter_map(ListingRequest::from_event)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ListingBatch {
    path: PathBuf,
    mode: fika_core::LoadMode,
    requests: Vec<ListingRequest>,
}

impl ListingBatch {
    fn read_events_cancellable(
        &self,
        state: &Arc<(Mutex<ListingWorkerState>, Condvar)>,
    ) -> Option<Vec<DirectoryListerEvent>> {
        let request = self.requests.first()?;
        DirectoryLister::read_listing_events_cancellable(
            request.pane_id,
            request.generation,
            request.request_serial,
            self.path.clone(),
            self.mode,
            || listing_batch_cancelled(state, self),
        )
    }
}

#[derive(Debug, Default)]
struct ListingWorkerState {
    pending: VecDeque<ListingRequest>,
    latest_request_by_pane: HashMap<PaneId, ListingRequestKey>,
    results_by_pane: BTreeMap<PaneId, Vec<DirectoryListerEvent>>,
    cache: DirectoryCache,
    shutdown: bool,
}

impl ListingWorkerState {
    fn schedule(&mut self, request: ListingRequest) {
        if request.mode == fika_core::LoadMode::Reload {
            self.cache.mark_stale(&request.path);
        }
        self.pending
            .retain(|pending| pending.pane_id != request.pane_id);
        self.latest_request_by_pane
            .insert(request.pane_id, request.key());
        self.results_by_pane.remove(&request.pane_id);
        self.pending.push_back(request);
    }

    fn cancel_pane(&mut self, pane_id: PaneId) {
        self.pending.retain(|pending| pending.pane_id != pane_id);
        self.latest_request_by_pane.remove(&pane_id);
        self.results_by_pane.remove(&pane_id);
    }

    fn mark_cache_stale(&mut self, path: &Path) {
        self.cache.mark_stale(path);
    }

    fn remove_cached_directory(&mut self, path: &Path) {
        self.cache.remove(path);
    }

    fn cached_events_for(&mut self, request: &ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        if request.mode != fika_core::LoadMode::Load {
            return None;
        }
        let snapshot = self.cache.get(&request.path)?;
        if snapshot.state() != fika_core::DirectoryCacheState::Fresh {
            return None;
        }
        Some(vec![
            DirectoryListerEvent::ListingRefreshed {
                pane_id: request.pane_id,
                generation: request.generation,
                request_serial: request.request_serial,
                path: request.path.clone(),
                entries: Arc::clone(snapshot.entries()),
            },
            DirectoryListerEvent::ListingCompleted {
                pane_id: request.pane_id,
                generation: request.generation,
                request_serial: request.request_serial,
                path: request.path.clone(),
            },
        ])
    }

    fn schedule_or_cached(&mut self, request: ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        if let Some(events) = self.cached_events_for(&request) {
            self.pending
                .retain(|pending| pending.pane_id != request.pane_id);
            self.latest_request_by_pane
                .insert(request.pane_id, request.key());
            self.results_by_pane.remove(&request.pane_id);
            return Some(events);
        }

        self.schedule(request);
        None
    }

    fn pop_batch(&mut self) -> Option<ListingBatch> {
        while let Some(leader) = self.pending.pop_front() {
            if !self.is_current(&leader) {
                continue;
            }

            let path = leader.path.clone();
            let mode = leader.mode;
            let mut requests = vec![leader];
            let mut index = 0;
            while index < self.pending.len() {
                let Some(pending) = self.pending.get(index) else {
                    break;
                };
                if !self.is_current(pending) {
                    self.pending.remove(index);
                    continue;
                }
                if pending.path == path && pending.mode == mode {
                    if let Some(request) = self.pending.remove(index) {
                        requests.push(request);
                    }
                    continue;
                }
                index += 1;
            }

            return Some(ListingBatch {
                path,
                mode,
                requests,
            });
        }
        None
    }

    fn is_current(&self, request: &ListingRequest) -> bool {
        self.latest_request_by_pane
            .get(&request.pane_id)
            .is_some_and(|key| *key == request.key())
    }

    fn publish_batch_if_current(
        &mut self,
        batch: &ListingBatch,
        events: &[DirectoryListerEvent],
    ) -> bool {
        if self.shutdown {
            return false;
        }
        let mut published = false;
        for request in &batch.requests {
            if !self.is_current(request) {
                continue;
            }
            self.results_by_pane
                .insert(request.pane_id, retarget_listing_events(events, request));
            published = true;
        }
        if published && let Some(entries) = listing_refreshed_entries(events) {
            self.cache.insert_fresh(&batch.path, entries);
        }
        published
    }

    fn drain_results(&mut self) -> Vec<Vec<DirectoryListerEvent>> {
        std::mem::take(&mut self.results_by_pane)
            .into_values()
            .collect()
    }
}

struct ListingWorker {
    state: Arc<(Mutex<ListingWorkerState>, Condvar)>,
    handle: Option<JoinHandle<()>>,
}

impl ListingWorker {
    fn new() -> Self {
        let state = Arc::new((Mutex::new(ListingWorkerState::default()), Condvar::new()));
        let worker_state = Arc::clone(&state);
        let handle = std::thread::spawn(move || listing_worker_loop(worker_state));
        Self {
            state,
            handle: Some(handle),
        }
    }

    fn schedule_all(&self, requests: Vec<ListingRequest>) {
        if requests.is_empty() {
            return;
        }
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return;
        }
        for request in requests {
            state.schedule(request);
        }
        cvar.notify_one();
    }

    fn schedule_or_cached(&self, request: ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return None;
        }
        let cached_events = state.schedule_or_cached(request);
        if cached_events.is_none() {
            cvar.notify_one();
        }
        cached_events
    }

    fn mark_cache_stale(&self, path: &Path) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.mark_cache_stale(path);
    }

    fn remove_cached_directory(&self, path: &Path) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.remove_cached_directory(path);
    }

    fn cancel_pane(&self, pane_id: PaneId) {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.cancel_pane(pane_id);
        cvar.notify_one();
    }

    fn drain_results(&self) -> Vec<Vec<DirectoryListerEvent>> {
        let (lock, _) = &*self.state;
        lock.lock()
            .expect("listing worker state poisoned")
            .drain_results()
    }
}

impl Drop for ListingWorker {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.pending.clear();
            state.results_by_pane.clear();
            cvar.notify_one();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn listing_batch_cancelled(
    state: &Arc<(Mutex<ListingWorkerState>, Condvar)>,
    batch: &ListingBatch,
) -> bool {
    let (lock, _) = &**state;
    let guard = lock.lock().expect("listing worker state poisoned");
    guard.shutdown
        || !batch
            .requests
            .iter()
            .any(|request| guard.is_current(request))
}

fn retarget_listing_events(
    events: &[DirectoryListerEvent],
    target: &ListingRequest,
) -> Vec<DirectoryListerEvent> {
    events
        .iter()
        .map(|event| retarget_listing_event(event, target))
        .collect()
}

fn retarget_listing_event(
    event: &DirectoryListerEvent,
    target: &ListingRequest,
) -> DirectoryListerEvent {
    match event {
        DirectoryListerEvent::LoadingStarted { .. } => DirectoryListerEvent::LoadingStarted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            mode: target.mode,
        },
        DirectoryListerEvent::ItemsAdded { entries, .. } => DirectoryListerEvent::ItemsAdded {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            entries: entries.clone(),
        },
        DirectoryListerEvent::ItemsDeleted { paths, .. } => DirectoryListerEvent::ItemsDeleted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            paths: paths.clone(),
        },
        DirectoryListerEvent::ItemsRefreshed { pairs, .. } => {
            DirectoryListerEvent::ItemsRefreshed {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
                pairs: pairs.clone(),
            }
        }
        DirectoryListerEvent::ListingRefreshed { entries, .. } => {
            DirectoryListerEvent::ListingRefreshed {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
                entries: Arc::clone(entries),
            }
        }
        DirectoryListerEvent::ListingCompleted { .. } => DirectoryListerEvent::ListingCompleted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
        },
        DirectoryListerEvent::CurrentDirectoryRemoved { .. } => {
            DirectoryListerEvent::CurrentDirectoryRemoved {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
            }
        }
        DirectoryListerEvent::Error { message, .. } => DirectoryListerEvent::Error {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            message: message.clone(),
        },
    }
}

fn listing_refreshed_entries(
    events: &[DirectoryListerEvent],
) -> Option<Arc<Vec<fika_core::Entry>>> {
    events.iter().find_map(|event| {
        if let DirectoryListerEvent::ListingRefreshed { entries, .. } = event {
            Some(Arc::clone(entries))
        } else {
            None
        }
    })
}

fn update_loading_state_for_event(
    loading_panes: &mut HashMap<PaneId, LoadingPaneState>,
    pane: Option<&fika_core::PaneState>,
    event: &DirectoryListerEvent,
    now: Instant,
) {
    match event {
        DirectoryListerEvent::LoadingStarted {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            if pane.is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            }) {
                loading_panes.insert(
                    *pane_id,
                    LoadingPaneState {
                        key: ListingRequestKey {
                            generation: *generation,
                            request_serial: *request_serial,
                        },
                        started_at: now,
                    },
                );
            } else {
                loading_panes.remove(pane_id);
            }
        }
        DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::Error {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::CurrentDirectoryRemoved {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            let key = ListingRequestKey {
                generation: *generation,
                request_serial: *request_serial,
            };
            if loading_panes
                .get(pane_id)
                .is_some_and(|state| state.key == key)
            {
                loading_panes.remove(pane_id);
            }
        }
        DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsAdded {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsDeleted {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsRefreshed {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            let Some(pane) = pane else {
                loading_panes.remove(pane_id);
                return;
            };
            if pane.generation != *generation
                || loading_panes
                    .get(pane_id)
                    .is_some_and(|state| state.key.request_serial < *request_serial)
            {
                loading_panes.remove(pane_id);
            }
        }
    }
}

fn listing_worker_loop(state: Arc<(Mutex<ListingWorkerState>, Condvar)>) {
    loop {
        let batch = {
            let (lock, cvar) = &*state;
            let mut guard = lock.lock().expect("listing worker state poisoned");
            while guard.pending.is_empty() && !guard.shutdown {
                guard = cvar.wait(guard).expect("listing worker state poisoned");
            }
            if guard.shutdown {
                return;
            }
            guard
                .pop_batch()
                .expect("pending listing request disappeared")
        };

        let Some(events) = batch.read_events_cancellable(&state) else {
            continue;
        };
        let (lock, _) = &*state;
        let mut guard = lock.lock().expect("listing worker state poisoned");
        if guard.shutdown {
            return;
        }
        guard.publish_batch_if_current(&batch, &events);
    }
}

pub(crate) struct FikaApp {
    pub(crate) panes: PaneController,
    places: Vec<PlaceEntry>,
    hidden_place_sections: BTreeSet<&'static str>,
    user_places_path: PathBuf,
    file_icons: FileIconCache,
    space_info: SpaceInfoCache,
    status_summaries: HashMap<PaneId, StatusSummaryCacheEntry>,
    loading_panes: HashMap<PaneId, LoadingPaneState>,
    smooth_scrolls: HashMap<PaneId, SmoothScroll>,
    scroll_drag_trackers: HashMap<PaneId, ScrollDragTracker>,
    smooth_scroll_tick_running: bool,
    viewport_origins: HashMap<PaneId, ViewPoint>,
    visible_item_slots: HashMap<PaneId, VisibleItemSlotPool>,
    compact_column_widths: HashMap<PaneId, CompactColumnWidthCache>,
    pane_filters: HashMap<PaneId, PaneFilterState>,
    filtered_models: HashMap<PaneId, FilteredModelCacheEntry>,
    operations: OperationQueue,
    clipboard: Option<ClipboardState>,
    rename_draft: Option<RenameDraft>,
    location_draft: Option<LocationDraft>,
    place_draft: Option<PlaceDraft>,
    chooser: Option<ChooserState>,
    listing_worker: ListingWorker,
    _keystroke_subscription: Option<gpui::Subscription>,
    pub(crate) rubber_band: Option<RubberBandState>,
    context_menu: Option<ContextMenuState>,
    properties_dialog: Option<PropertiesDialogState>,
    pane_statuses: HashMap<PaneId, String>,
    operation_pending: bool,
    operation_pane: Option<PaneId>,
    operation_progress: Option<OperationProgressHandle>,
}

impl FikaApp {
    fn new(args: Args, cx: &mut Context<Self>) -> Self {
        let user_places_path = fika_core::default_user_places_path();
        let chooser = (args.mode == Mode::Chooser).then(|| ChooserState {
            directories: args.chooser_directories,
            multiple: args.chooser_multiple,
            title: args
                .chooser_title
                .clone()
                .unwrap_or_else(|| "Fika File Chooser".to_string()),
            accept_label: args
                .chooser_accept_label
                .clone()
                .unwrap_or_else(|| "Choose".to_string()),
            filter_index: args.chooser_filter_index,
            return_filter: args.chooser_return_filter,
            choices: args.chooser_choices.clone(),
            return_choices: args.chooser_return_choices,
        });
        let mut app = Self {
            panes: PaneController::new(args.start_dir.clone()),
            places: build_places(&user_places_path),
            hidden_place_sections: BTreeSet::new(),
            user_places_path,
            file_icons: FileIconCache::default(),
            space_info: SpaceInfoCache::default(),
            status_summaries: HashMap::new(),
            loading_panes: HashMap::new(),
            smooth_scrolls: HashMap::new(),
            scroll_drag_trackers: HashMap::new(),
            smooth_scroll_tick_running: false,
            viewport_origins: HashMap::new(),
            visible_item_slots: HashMap::new(),
            compact_column_widths: HashMap::new(),
            pane_filters: HashMap::new(),
            filtered_models: HashMap::new(),
            operations: OperationQueue::new(),
            clipboard: None,
            rename_draft: None,
            location_draft: None,
            place_draft: None,
            chooser,
            listing_worker: ListingWorker::new(),
            _keystroke_subscription: None,
            rubber_band: None,
            context_menu: None,
            properties_dialog: None,
            pane_statuses: HashMap::new(),
            operation_pending: false,
            operation_pane: None,
            operation_progress: None,
        };
        app._keystroke_subscription = Some(cx.observe_keystrokes(|this, event, _window, cx| {
            if this.handle_keystroke(event, cx) {
                cx.notify();
            }
        }));
        let first = app.panes.focused().expect("initial pane exists");
        app.load_pane(first, args.start_dir);
        app.start_watchers();
        cx.spawn(|this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(350))
                        .await;
                    if this
                        .update(&mut cx, |app, cx| {
                            if app.drain_background_listing_results()
                                | app.drain_watchers()
                                | app.operation_progress.is_some()
                                | !app.loading_panes.is_empty()
                            {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })
        .detach();
        app
    }

    fn active_filter_for_pane(&self, pane_id: PaneId) -> Option<fika_core::NameFilter> {
        self.pane_filters
            .get(&pane_id)
            .and_then(PaneFilterState::active_filter)
    }

    fn filtered_model_for_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Option<(fika_core::FilteredModel, u64)> {
        let Some(filter) = self.active_filter_for_pane(pane_id) else {
            self.filtered_models.remove(&pane_id);
            return None;
        };
        let source_revision = filter_source_revision(&filter);
        let model_generation = self.panes.pane(pane_id)?.model.data_generation();
        let key = FilteredModelCacheKey {
            model_generation,
            filter: filter.clone(),
        };
        if let Some(cached) = self
            .filtered_models
            .get(&pane_id)
            .filter(|cached| cached.key == key)
        {
            return Some((cached.model.clone(), source_revision));
        }

        let model = {
            let pane = self.panes.pane(pane_id)?;
            fika_core::FilteredModel::from_model(&pane.model, &filter)
        };
        self.filtered_models.insert(
            pane_id,
            FilteredModelCacheEntry {
                key,
                model: model.clone(),
            },
        );
        Some((model, source_revision))
    }

    fn filter_bar_snapshot(
        &self,
        pane_id: PaneId,
        focused_pane: Option<PaneId>,
        match_count: usize,
    ) -> Option<FilterBarSnapshot> {
        let filter = self
            .pane_filters
            .get(&pane_id)
            .filter(|filter| filter.visible)?;
        Some(FilterBarSnapshot {
            query: filter.query.clone(),
            focused: filter.focused && focused_pane == Some(pane_id),
            case_sensitive: filter.case_sensitive,
            mode: filter.mode,
            match_count,
        })
    }

    pub(crate) fn show_filter_bar(&mut self, pane_id: PaneId) {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        self.set_pane_status(pane_id, "Filter");
    }

    pub(crate) fn focus_filter_bar(&mut self, pane_id: PaneId) {
        self.show_filter_bar(pane_id);
    }

    pub(crate) fn close_filter_bar(&mut self, pane_id: PaneId) {
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.visible = false;
            filter.focused = false;
            filter.query.clear();
        }
        self.invalidate_filter_projection(pane_id);
        self.set_pane_status(pane_id, "Filter closed");
    }

    fn set_filter_query(&mut self, pane_id: PaneId, query: String) {
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        if filter.query == query {
            return;
        }
        filter.query = query;
        self.invalidate_filter_projection(pane_id);
        self.set_pane_status(pane_id, "Filtering");
    }

    pub(crate) fn toggle_filter_case_sensitive(&mut self, pane_id: PaneId) {
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        filter.case_sensitive = !filter.case_sensitive;
        let enabled = filter.case_sensitive;
        self.invalidate_filter_projection(pane_id);
        let message = if enabled {
            "Filter match case"
        } else {
            "Filter ignore case"
        };
        self.set_pane_status(pane_id, message);
    }

    pub(crate) fn toggle_filter_mode(&mut self, pane_id: PaneId) {
        let filter = self.pane_filters.entry(pane_id).or_default();
        filter.visible = true;
        filter.focused = true;
        filter.mode = match filter.mode {
            fika_core::NameFilterMode::PlainText => fika_core::NameFilterMode::Glob,
            fika_core::NameFilterMode::Glob => fika_core::NameFilterMode::PlainText,
        };
        let mode = filter.mode;
        self.invalidate_filter_projection(pane_id);
        let message = match mode {
            fika_core::NameFilterMode::PlainText => "Plain text filter",
            fika_core::NameFilterMode::Glob => "Glob filter",
        };
        self.set_pane_status(pane_id, message);
    }

    fn clear_filter_query_for_pane(&mut self, pane_id: PaneId) {
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.query.clear();
        }
        self.invalidate_filter_projection(pane_id);
    }

    fn clear_filter_query_for_url_change(&mut self, pane_id: PaneId) {
        let Some(filter) = self.pane_filters.get_mut(&pane_id) else {
            return;
        };
        if filter.query.is_empty() {
            return;
        }
        filter.query.clear();
        filter.focused = false;
        self.invalidate_pane_layout_projection(pane_id, false);
    }

    fn invalidate_filter_projection(&mut self, pane_id: PaneId) {
        self.invalidate_pane_layout_projection(pane_id, true);
    }

    fn invalidate_pane_layout_projection(&mut self, pane_id: PaneId, reset_scroll: bool) {
        self.visible_item_slots.remove(&pane_id);
        self.compact_column_widths.remove(&pane_id);
        self.filtered_models.remove(&pane_id);
        self.status_summaries.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
        self.scroll_drag_trackers.remove(&pane_id);
        if let Some(pane) = self.panes.pane_mut(pane_id) {
            if reset_scroll {
                pane.view.reset_scroll();
            }
        }
    }

    fn clear_filter_focus_for_pane(&mut self, pane_id: PaneId) {
        if let Some(filter) = self.pane_filters.get_mut(&pane_id) {
            filter.focused = false;
        }
    }

    fn handle_filter_keystroke(&mut self, pane_id: PaneId, keystroke: &gpui::Keystroke) -> bool {
        if !self
            .pane_filters
            .get(&pane_id)
            .is_some_and(|filter| filter.visible && filter.focused)
        {
            return false;
        }

        match filter_input_action(keystroke) {
            FilterInputAction::Cancel => {
                let query_empty = self
                    .pane_filters
                    .get(&pane_id)
                    .is_none_or(|filter| filter.query.is_empty());
                if query_empty {
                    self.close_filter_bar(pane_id);
                } else {
                    self.clear_filter_query_for_pane(pane_id);
                    self.set_pane_status(pane_id, "Filter cleared");
                }
            }
            FilterInputAction::FocusView => {
                self.clear_filter_focus_for_pane(pane_id);
            }
            FilterInputAction::Backspace => {
                let next = self
                    .pane_filters
                    .get(&pane_id)
                    .map(|filter| {
                        let mut query = filter.query.clone();
                        query.pop();
                        query
                    })
                    .unwrap_or_default();
                self.set_filter_query(pane_id, next);
            }
            FilterInputAction::Insert(text) => {
                let mut next = self
                    .pane_filters
                    .get(&pane_id)
                    .map(|filter| filter.query.clone())
                    .unwrap_or_default();
                next.push_str(&text);
                self.set_filter_query(pane_id, next);
            }
            FilterInputAction::PassToView => {
                self.clear_filter_focus_for_pane(pane_id);
                return false;
            }
            FilterInputAction::Ignore => return false,
        }
        true
    }

    fn snapshots(&mut self, cx: &mut Context<Self>) -> Vec<PaneSnapshot> {
        let focused_pane = self.panes.focused();
        let pane_ids = self.panes.pane_ids().to_vec();
        pane_ids
            .into_iter()
            .filter_map(|pane_id| {
                let filtered_model = self.filtered_model_for_pane(pane_id);
                let (
                    breadcrumbs,
                    location_draft,
                    filter_bar,
                    layout,
                    view,
                    rubber_band,
                    focused,
                    visible_data,
                ) = {
                    let pane = self.panes.pane(pane_id)?;
                    let filtered = filtered_model.as_ref().map(|(model, _)| model);
                    let source_revision =
                        filtered_model.as_ref().map_or(0, |(_, revision)| *revision);
                    let rename_draft = self
                        .rename_draft
                        .as_ref()
                        .filter(|draft| draft.pane_id == pane_id);
                    let location_draft = self
                        .location_draft
                        .as_ref()
                        .filter(|draft| draft.pane_id == pane_id)
                        .map(|draft| draft.value.clone());
                    let layout = match filtered {
                        Some(filtered) => compact_layout_for_filtered_model(
                            self.compact_column_widths.entry(pane_id).or_default(),
                            &pane.model,
                            filtered,
                            source_revision,
                            &pane.view,
                        ),
                        None => compact_layout_for_model(
                            self.compact_column_widths.entry(pane_id).or_default(),
                            &pane.model,
                            &pane.view,
                        ),
                    };
                    let visible_data = layout
                        .visible_items()
                        .filter_map(|visible_item| {
                            let layout_index = visible_item.model_index;
                            let model_index = model_index_for_layout_index(filtered, layout_index)?;
                            let entry = pane.model.get(model_index)?;
                            let path = pane.model.path_for_index(model_index)?;
                            let item_layout = layout.item_with_required_text_width(
                                layout_index,
                                Some(compact_text_width(entry.name_width_units)),
                            )?;
                            let selected = pane.selection.is_selected(entry.id);
                            let draft_name = rename_draft
                                .filter(|draft| draft.original_path == path)
                                .map(|draft| draft.draft_name.clone());
                            Some((
                                item_layout,
                                entry.id,
                                path,
                                entry.is_dir,
                                entry.name.clone(),
                                format_entry_kind_label(entry),
                                selected,
                                draft_name,
                            ))
                        })
                        .collect::<Vec<_>>();
                    (
                        breadcrumb_segments(&pane.current_dir),
                        location_draft,
                        self.filter_bar_snapshot(
                            pane_id,
                            focused_pane,
                            filtered
                                .map_or_else(|| pane.model.len(), fika_core::FilteredModel::len),
                        ),
                        layout,
                        pane.view.clone(),
                        self.rubber_band.and_then(|band| {
                            (band.pane_id == pane_id)
                                .then(|| band.visible_rect(pane.view.scroll_x, pane.view.scroll_y))
                        }),
                        focused_pane == Some(pane_id),
                        visible_data,
                    )
                };
                let visible_ids = visible_data
                    .iter()
                    .map(|(_, item_id, _, _, _, _, _, _)| *item_id);
                let slot_by_item_id = self
                    .visible_item_slots
                    .entry(pane_id)
                    .or_default()
                    .slots_for_items(visible_ids);
                let visible_items = visible_data
                    .into_iter()
                    .filter_map(
                        |(
                            layout,
                            item_id,
                            path,
                            is_dir,
                            name,
                            kind_label,
                            selected,
                            draft_name,
                        )| {
                            let slot_id = slot_by_item_id.get(&item_id).copied()?;
                            let icon = self.file_icons.icon_for(&path, is_dir);
                            Some(VisibleItemSnapshot {
                                slot_id,
                                layout,
                                path,
                                is_dir,
                                name,
                                kind_label,
                                icon,
                                selected,
                                draft_name,
                            })
                        },
                    )
                    .collect::<Vec<_>>();
                let status_bar = self.status_bar_snapshot_for_pane(pane_id, cx);
                Some(PaneSnapshot {
                    id: pane_id,
                    breadcrumbs,
                    location_draft,
                    filter_bar,
                    status_bar,
                    layout,
                    visible_items,
                    view,
                    rubber_band,
                    focused,
                })
            })
            .collect()
    }

    fn status_bar_snapshot_for_pane(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> StatusBarSnapshot {
        let now = Instant::now();
        let message = self.status_message_for_pane(pane_id);
        let operation_pending = self.operation_pane == Some(pane_id) && self.operation_pending;
        let Some((path, zoom_level, zoom_icon_size)) = self.panes.pane(pane_id).map(|pane| {
            (
                pane.current_dir.clone(),
                pane.view.zoom_level,
                pane.view.icon_size(),
            )
        }) else {
            return StatusBarSnapshot {
                message,
                item_summary: "0 folders, 0 files".to_string(),
                free_space: None,
                zoom_level: fika_core::DEFAULT_ZOOM_LEVEL,
                zoom_icon_size: fika_core::icon_size_for_zoom_level(fika_core::DEFAULT_ZOOM_LEVEL),
                zoom_min: fika_core::MIN_ZOOM_LEVEL,
                zoom_max: fika_core::MAX_ZOOM_LEVEL,
                operation_pending,
                operation_progress: self.operation_progress_snapshot_for_pane(pane_id, now),
            };
        };

        self.request_space_info_if_needed(path.clone(), cx);
        let operation_progress = self
            .operation_progress_snapshot_for_pane(pane_id, now)
            .or_else(|| self.loading_progress_snapshot(pane_id, now));
        StatusBarSnapshot {
            message,
            item_summary: self
                .status_summary_for_pane(pane_id)
                .unwrap_or_else(|| "0 folders, 0 files".to_string()),
            free_space: self.space_info.snapshot_for(&path),
            zoom_level,
            zoom_icon_size,
            zoom_min: fika_core::MIN_ZOOM_LEVEL,
            zoom_max: fika_core::MAX_ZOOM_LEVEL,
            operation_pending,
            operation_progress,
        }
    }

    fn status_message_for_pane(&self, pane_id: PaneId) -> String {
        self.pane_statuses
            .get(&pane_id)
            .filter(|message| !message.is_empty())
            .cloned()
            .unwrap_or_else(|| "Ready".to_string())
    }

    fn set_pane_status(&mut self, pane_id: PaneId, message: impl Into<String>) {
        self.pane_statuses.insert(pane_id, message.into());
    }

    fn begin_pane_operation(&mut self, pane_id: PaneId, message: impl Into<String>) {
        self.operation_pending = true;
        self.operation_pane = Some(pane_id);
        self.set_pane_status(pane_id, message);
    }

    fn finish_pane_operation(&mut self, pane_id: PaneId, message: impl Into<String>) {
        self.operation_pending = false;
        self.operation_pane = None;
        self.set_pane_status(pane_id, message);
    }

    fn operation_progress_snapshot_for_pane(
        &self,
        pane_id: PaneId,
        now: Instant,
    ) -> Option<OperationProgressSnapshot> {
        self.operation_progress
            .as_ref()
            .filter(|progress| progress.pane_id == pane_id)
            .and_then(|progress| progress.snapshot(now))
    }

    fn loading_progress_snapshot(
        &self,
        pane_id: PaneId,
        now: Instant,
    ) -> Option<OperationProgressSnapshot> {
        self.loading_panes.get(&pane_id).and_then(|loading| {
            progress_delay_elapsed(loading.started_at, now).then(|| OperationProgressSnapshot {
                label: "Loading".to_string(),
                bytes_done: 0,
                bytes_total: 0,
                percent: None,
                cancellable: true,
            })
        })
    }

    fn start_transfer_progress(
        &mut self,
        pane_id: PaneId,
        label: String,
    ) -> (Arc<AtomicBool>, Arc<Mutex<file_ops::TransferProgress>>) {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(file_ops::TransferProgress::default()));
        self.operation_progress = Some(OperationProgressHandle {
            pane_id,
            label,
            progress: Arc::clone(&progress),
            cancel: Some(Arc::clone(&cancel)),
            started_at: Instant::now(),
        });
        (cancel, progress)
    }

    fn clear_operation_progress(&mut self) {
        self.operation_progress = None;
    }

    pub(crate) fn cancel_operation_or_loading(&mut self, pane_id: PaneId) {
        if let Some(progress) = &self.operation_progress
            && progress.pane_id == pane_id
            && let Some(cancel) = &progress.cancel
        {
            cancel.store(true, Ordering::Relaxed);
            self.set_pane_status(pane_id, format!("Cancelling {}", progress.label));
            return;
        }
        self.cancel_loading(pane_id);
    }

    pub(crate) fn cancel_loading(&mut self, pane_id: PaneId) {
        if self.loading_panes.remove(&pane_id).is_some() {
            self.listing_worker.cancel_pane(pane_id);
            self.set_pane_status(pane_id, "Loading stopped");
        }
    }

    fn status_summary_for_pane(&mut self, pane_id: PaneId) -> Option<String> {
        let filtered = self.filtered_model_for_pane(pane_id);
        let (key, summary) = {
            let pane = self.panes.pane(pane_id)?;
            let filter_revision = filtered.as_ref().map_or(0, |(_, revision)| *revision);
            let visible_len = filtered
                .as_ref()
                .map_or_else(|| pane.model.len(), |(filtered, _)| filtered.len());
            let selection_count = pane.selection.count_for_model(pane.model.len());
            let key = StatusSummaryCacheKey {
                model_generation: pane.model.data_generation(),
                model_len: pane.model.len(),
                filter_revision,
                visible_len,
                selection_count,
                selection_revision: pane.selection.revision(),
            };
            if let Some(cached) = self
                .status_summaries
                .get(&pane_id)
                .filter(|cached| cached.key == key)
            {
                return Some(cached.summary.clone());
            }
            let summary = match filtered {
                Some((filtered, _)) if pane.selection.is_empty() => {
                    status_summary_for_model_indexes(
                        pane.model.entries(),
                        filtered.iter_model_indexes(),
                        &pane.selection,
                    )
                }
                _ => status_summary_for_model(pane.model.entries(), &pane.selection),
            };
            (key, summary)
        };
        self.status_summaries.insert(
            pane_id,
            StatusSummaryCacheEntry {
                key,
                summary: summary.clone(),
            },
        );
        Some(summary)
    }

    fn request_space_info_if_needed(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let now = Instant::now();
        if !self.space_info.should_request(&path, now) {
            return;
        }
        self.space_info.start_request(path.clone(), now);

        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let request_path = path.clone();
                    let snapshot = cx
                        .background_spawn(async move { filesystem_space_info(request_path) })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        if app.finish_space_info_request(path, snapshot) {
                            cx.notify();
                        }
                    });
                }
            },
        )
        .detach();
    }

    fn finish_space_info_request(
        &mut self,
        path: PathBuf,
        snapshot: Option<SpaceInfoSnapshot>,
    ) -> bool {
        self.space_info.finish_request(&path, snapshot)
    }

    fn place_snapshots(&self) -> Vec<PlaceSnapshot> {
        let current_dir = self
            .panes
            .focused()
            .and_then(|pane_id| self.panes.pane(pane_id))
            .map(|pane| pane.current_dir.as_path());
        let active_index = current_dir.and_then(|path| active_place_index(&self.places, path));

        self.places
            .iter()
            .enumerate()
            .filter(|(_, place)| !self.hidden_place_sections.contains(place.group))
            .map(|(index, place)| {
                let trash_place = file_ops::is_trash_files_dir(&place.path);
                PlaceSnapshot {
                    group: place.group,
                    marker: place.marker,
                    label: place.label.clone(),
                    path: place.path.clone(),
                    active: active_index == Some(index),
                    trash_place,
                    trash_has_items: trash_place && file_ops::trash_has_items(),
                    editable: place.editable,
                    removable: place.removable,
                }
            })
            .collect()
    }

    fn open_place(&mut self, path: PathBuf) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        if path == file_ops::trash_files_dir() {
            let _ = file_ops::ensure_trash_dirs();
        }
        self.load_pane(pane_id, path);
    }

    pub(crate) fn show_place_context_menu(
        &mut self,
        place: PlaceSnapshot,
        position: gpui::Point<gpui::Pixels>,
    ) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.context_menu = Some(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Place {
                path: place.path,
                trash_place: place.trash_place,
                trash_has_items: place.trash_has_items,
                editable: place.editable,
                removable: place.removable,
            },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    pub(crate) fn show_place_section_context_menu(
        &mut self,
        group: &'static str,
        position: gpui::Point<gpui::Pixels>,
    ) {
        if group.is_empty() || !self.places.iter().any(|place| place.group == group) {
            return;
        }
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.context_menu = Some(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::PlaceSection { group },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    pub(crate) fn show_places_blank_context_menu(&mut self, position: gpui::Point<gpui::Pixels>) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.context_menu = Some(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::PlacesBlank {
                hidden_sections: !self.hidden_place_sections.is_empty(),
            },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    fn load_pane(&mut self, pane_id: PaneId, path: PathBuf) {
        let url_changed = self
            .panes
            .pane(pane_id)
            .is_some_and(|pane| pane.current_dir != path);
        let Some(event) = self.panes.load(pane_id, path.clone()) else {
            return;
        };
        self.clear_pane_transient_state(pane_id);
        if url_changed {
            self.clear_filter_query_for_url_change(pane_id);
        }
        let cached_events = self.schedule_listing(&event);
        self.apply_event(event);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
    }

    fn reload_pane(&mut self, pane_id: PaneId) {
        let Some(event) = self.panes.reload(pane_id) else {
            return;
        };
        self.clear_pane_transient_state(pane_id);
        let cached_events = self.schedule_listing(&event);
        self.apply_event(event);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        if let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        {
            self.set_pane_status(pane_id, format!("Reloading {}", path.display()));
        }
    }

    fn go_back(&mut self, pane_id: PaneId) {
        let Some(event) = self.panes.go_back(pane_id) else {
            return;
        };
        self.clear_pane_transient_state(pane_id);
        self.clear_filter_query_for_url_change(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.apply_event(event);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
    }

    fn go_forward(&mut self, pane_id: PaneId) {
        let Some(event) = self.panes.go_forward(pane_id) else {
            return;
        };
        self.clear_pane_transient_state(pane_id);
        self.clear_filter_query_for_url_change(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.apply_event(event);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.set_pane_status(pane_id, format!("Loading {}", path.display()));
    }

    fn go_parent(&mut self, pane_id: PaneId) {
        let Some(parent) = self
            .panes
            .pane(pane_id)
            .and_then(|pane| pane.current_dir.parent().map(Path::to_path_buf))
        else {
            return;
        };
        self.load_pane(pane_id, parent);
    }

    fn split_pane(&mut self, pane_id: PaneId) {
        let Some(new_id) = self.panes.split(pane_id) else {
            return;
        };
        self.start_watcher(new_id);
        self.set_pane_status(new_id, format!("Split pane {}", new_id.0));
    }

    fn open_path_in_new_pane(&mut self, source_pane_id: PaneId, path: PathBuf) {
        let Some(new_id) = self.panes.split(source_pane_id) else {
            return;
        };
        self.load_pane(new_id, path);
    }

    fn close_pane(&mut self, pane_id: PaneId) {
        if self.panes.close(pane_id) {
            self.listing_worker.cancel_pane(pane_id);
            self.clear_pane_transient_state(pane_id);
            self.pane_filters.remove(&pane_id);
            if let Some(focused_pane) = self.panes.focused() {
                self.set_pane_status(focused_pane, format!("Closed pane {}", pane_id.0));
            }
        }
    }

    fn clear_pane_transient_state(&mut self, pane_id: PaneId) {
        self.visible_item_slots.remove(&pane_id);
        self.compact_column_widths.remove(&pane_id);
        self.status_summaries.remove(&pane_id);
        self.filtered_models.remove(&pane_id);
        self.loading_panes.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
        self.scroll_drag_trackers.remove(&pane_id);
        self.viewport_origins.remove(&pane_id);
        self.pane_statuses.remove(&pane_id);
        if self
            .rubber_band
            .as_ref()
            .is_some_and(|band| band.pane_id == pane_id)
        {
            self.rubber_band = None;
        }
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.pane_id == pane_id)
        {
            self.context_menu = None;
        }
        self.properties_dialog = None;
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
    }

    fn select_only(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.select_only(pane_id, path) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.toggle_selection(pane_id, path).is_some() {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn select_range_to(&mut self, pane_id: PaneId, path: PathBuf) {
        let selected = if let Some((filtered, _)) = self.filtered_model_for_pane(pane_id) {
            self.select_filtered_range_to(pane_id, &filtered, path)
        } else {
            self.panes.select_range_to(pane_id, path)
        };
        if let Some(selected) = selected {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn select_all(&mut self, pane_id: PaneId) {
        let selected = if let Some((filtered, _)) = self.filtered_model_for_pane(pane_id) {
            self.select_all_filtered(pane_id, &filtered)
        } else {
            self.panes.select_all(pane_id)
        };
        if let Some(selected) = selected {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn clear_selection(&mut self, pane_id: PaneId) {
        if self.panes.clear_selection(pane_id) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, "Selection cleared");
        }
    }

    fn move_selection(&mut self, pane_id: PaneId, direction: SelectionMove, extend: bool) {
        let selected = if let Some((filtered, _)) = self.filtered_model_for_pane(pane_id) {
            self.move_filtered_selection(pane_id, &filtered, direction, extend)
        } else {
            self.panes.move_selection(pane_id, direction, extend)
        };
        if let Some(selected) = selected {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn select_all_filtered(
        &mut self,
        pane_id: PaneId,
        filtered: &fika_core::FilteredModel,
    ) -> Option<usize> {
        let pane = self.panes.pane_mut(pane_id)?;
        let ids = filtered
            .iter_model_indexes()
            .filter_map(|index| pane.model.get(index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection.replace(ids);
        Some(count)
    }

    fn select_filtered_range_to(
        &mut self,
        pane_id: PaneId,
        filtered: &fika_core::FilteredModel,
        path: PathBuf,
    ) -> Option<usize> {
        let pane = self.panes.pane_mut(pane_id)?;
        let target_model_index = pane.model.index_of_path(&path)?;
        let target_layout_index = filtered.layout_index_for_model_index(target_model_index)?;
        let target_id = pane.model.get(target_model_index)?.id;
        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| {
                pane.model
                    .index_of_id(*id)
                    .and_then(|index| filtered.layout_index_for_model_index(index))
                    .is_some()
            })
            .unwrap_or(target_id);
        let anchor_layout_index = pane
            .model
            .index_of_id(anchor_id)
            .and_then(|index| filtered.layout_index_for_model_index(index))
            .unwrap_or(target_layout_index);
        let (start, end) = if anchor_layout_index <= target_layout_index {
            (anchor_layout_index, target_layout_index)
        } else {
            (target_layout_index, anchor_layout_index)
        };
        let ids = filtered.as_slice()[start..=end]
            .iter()
            .filter_map(|index| pane.model.get(*index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    fn move_filtered_selection(
        &mut self,
        pane_id: PaneId,
        filtered: &fika_core::FilteredModel,
        direction: SelectionMove,
        extend: bool,
    ) -> Option<usize> {
        if filtered.is_empty() {
            return None;
        }
        let pane = self.panes.pane_mut(pane_id)?;
        let current_layout_index = pane
            .selection
            .active_id()
            .and_then(|active| pane.model.index_of_id(active))
            .and_then(|index| filtered.layout_index_for_model_index(index))
            .or_else(|| {
                pane.selection
                    .selected_ids()
                    .last()
                    .and_then(|id| pane.model.index_of_id(*id))
                    .and_then(|index| filtered.layout_index_for_model_index(index))
            });
        let target_layout_index = match (current_layout_index, direction) {
            (Some(index), SelectionMove::Previous) => index.saturating_sub(1),
            (Some(index), SelectionMove::Next) => (index + 1).min(filtered.len() - 1),
            (None, SelectionMove::Previous) => filtered.len() - 1,
            (None, SelectionMove::Next) => 0,
        };
        let target_model_index = filtered.model_index(target_layout_index)?;
        let target_id = pane.model.get(target_model_index)?.id;

        if !extend {
            pane.selection.select_only(target_id);
            return Some(1);
        }

        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| {
                pane.model
                    .index_of_id(*id)
                    .and_then(|index| filtered.layout_index_for_model_index(index))
                    .is_some()
            })
            .unwrap_or(target_id);
        let anchor_layout_index = pane
            .model
            .index_of_id(anchor_id)
            .and_then(|index| filtered.layout_index_for_model_index(index))
            .unwrap_or(target_layout_index);
        let (start, end) = if anchor_layout_index <= target_layout_index {
            (anchor_layout_index, target_layout_index)
        } else {
            (target_layout_index, anchor_layout_index)
        };
        let ids = filtered.as_slice()[start..=end]
            .iter()
            .filter_map(|index| pane.model.get(*index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    fn apply_zoom_change(&mut self, pane_id: PaneId, change: ZoomChange) {
        let Some(previous_level) = self.panes.pane(pane_id).map(|pane| pane.view.zoom_level) else {
            return;
        };
        let Some(view) = self.panes.apply_zoom_change(pane_id, change) else {
            return;
        };
        if view.zoom_level == previous_level {
            self.set_pane_status(
                pane_id,
                format!(
                    "Zoom level {} ({} px)",
                    view.zoom_level,
                    view.icon_size() as i32
                ),
            );
            return;
        }
        self.compact_column_widths.remove(&pane_id);
        self.smooth_scrolls.remove(&pane_id);
        self.scroll_drag_trackers.remove(&pane_id);
        self.set_pane_status(
            pane_id,
            format!(
                "Zoom level {} ({} px)",
                view.zoom_level,
                view.icon_size() as i32
            ),
        );
    }

    pub(crate) fn set_zoom_level(&mut self, pane_id: PaneId, level: i32) {
        let Some(previous_level) = self.panes.pane(pane_id).map(|pane| pane.view.zoom_level) else {
            return;
        };
        let Some(view) = self.panes.set_zoom_level(pane_id, level) else {
            return;
        };
        if view.zoom_level != previous_level {
            self.compact_column_widths.remove(&pane_id);
            self.smooth_scrolls.remove(&pane_id);
            self.scroll_drag_trackers.remove(&pane_id);
        }
        self.set_pane_status(
            pane_id,
            format!(
                "Zoom level {} ({} px)",
                view.zoom_level,
                view.icon_size() as i32
            ),
        );
    }

    fn set_pane_sort_role(&mut self, pane_id: PaneId, role: SortRole) {
        let Some((sort, signals)) = self.panes.set_sort_role(pane_id, role) else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn set_pane_sort_order(&mut self, pane_id: PaneId, order: SortOrder) {
        let Some((sort, signals)) = self.panes.set_sort_order(pane_id, order) else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn set_pane_sort_folders_first(&mut self, pane_id: PaneId, folders_first: bool) {
        let Some((sort, signals)) = self.panes.set_sort_folders_first(pane_id, folders_first)
        else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn set_pane_sort_hidden_last(&mut self, pane_id: PaneId, hidden_last: bool) {
        let Some((sort, signals)) = self.panes.set_sort_hidden_last(pane_id, hidden_last) else {
            return;
        };
        self.finish_pane_sort(pane_id, sort, &signals);
    }

    fn finish_pane_sort(
        &mut self,
        pane_id: PaneId,
        sort: SortDescriptor,
        signals: &[fika_core::DirectoryModelSignal],
    ) {
        if !signals.is_empty() {
            self.invalidate_pane_layout_projection(pane_id, true);
        }
        self.set_pane_status(
            pane_id,
            format!(
                "Sorted by {} ({})",
                sort_role_label(sort.role),
                sort_order_label(sort.order)
            ),
        );
    }

    pub(crate) fn scroll_pane_smooth(
        &mut self,
        pane_id: PaneId,
        delta_x: f32,
        delta_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
        cx: &mut Context<Self>,
    ) {
        if delta_x.abs() <= f32::EPSILON && delta_y.abs() <= f32::EPSILON {
            return;
        }
        let Some(current) = self.panes.pane(pane_id).map(|pane| ViewPoint {
            x: pane.view.scroll_x,
            y: pane.view.scroll_y,
        }) else {
            return;
        };
        let bounds = ScrollBounds::new(max_scroll_x, max_scroll_y);
        let delta = ViewPoint {
            x: delta_x,
            y: delta_y,
        };
        let target = bounds.clamp(ViewPoint {
            x: current.x + delta.x,
            y: current.y + delta.y,
        });
        if target == current && !self.smooth_scrolls.contains_key(&pane_id) {
            return;
        }

        let now = Instant::now();
        let scroll = self.smooth_scrolls.remove(&pane_id).map_or_else(
            || SmoothScroll::to_target(current, target, bounds, now),
            |scroll| scroll.retarget(current, delta, bounds, now),
        );
        if scroll
            .target_offset()
            .is_some_and(|target| target == current)
        {
            return;
        }
        self.scroll_drag_trackers.remove(&pane_id);
        self.smooth_scrolls.insert(pane_id, scroll);
        self.schedule_smooth_scroll_tick(cx);
    }

    pub(crate) fn set_pane_scroll_immediate(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) {
        self.smooth_scrolls.remove(&pane_id);
        if let Some(view) =
            self.panes
                .set_view_scroll(pane_id, scroll_x, scroll_y, max_scroll_x, max_scroll_y)
        {
            self.scroll_drag_trackers
                .entry(pane_id)
                .or_default()
                .sample(
                    ViewPoint {
                        x: view.scroll_x,
                        y: view.scroll_y,
                    },
                    Instant::now(),
                );
        }
    }

    pub(crate) fn finish_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        max_scroll_x: f32,
        max_scroll_y: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(tracker) = self.scroll_drag_trackers.remove(&pane_id) else {
            return;
        };
        let bounds = ScrollBounds::new(max_scroll_x, max_scroll_y);
        if let Some(scroll) = SmoothScroll::kinetic(tracker.velocity(), bounds, Instant::now()) {
            self.smooth_scrolls.insert(pane_id, scroll);
            self.schedule_smooth_scroll_tick(cx);
        }
    }

    fn advance_smooth_scrolls(&mut self, now: Instant) -> bool {
        let pane_ids = self.smooth_scrolls.keys().copied().collect::<Vec<_>>();
        let mut changed = false;
        for pane_id in pane_ids {
            let Some(mut scroll) = self.smooth_scrolls.remove(&pane_id) else {
                continue;
            };
            let Some(current) = self.panes.pane(pane_id).map(|pane| ViewPoint {
                x: pane.view.scroll_x,
                y: pane.view.scroll_y,
            }) else {
                continue;
            };
            let bounds = scroll.bounds();
            let advance = scroll.advance(current, now);
            if let Some(view) = self.panes.set_view_scroll(
                pane_id,
                advance.offset.x,
                advance.offset.y,
                bounds.max_x,
                bounds.max_y,
            ) {
                changed |= (view.scroll_x - current.x).abs() > f32::EPSILON
                    || (view.scroll_y - current.y).abs() > f32::EPSILON;
            }
            if advance.active {
                self.smooth_scrolls.insert(pane_id, scroll);
            }
        }
        changed
    }

    fn schedule_smooth_scroll_tick(&mut self, cx: &mut Context<Self>) {
        if self.smooth_scroll_tick_running || self.smooth_scrolls.is_empty() {
            return;
        }
        self.smooth_scroll_tick_running = true;
        cx.spawn(|this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
            let mut cx = cx.clone();
            async move {
                loop {
                    cx.background_executor().timer(SMOOTH_SCROLL_FRAME).await;
                    let Ok(keep_running) = this.update(&mut cx, |app, cx| {
                        let changed = app.advance_smooth_scrolls(Instant::now());
                        if changed {
                            cx.notify();
                        }
                        if app.smooth_scrolls.is_empty() {
                            app.smooth_scroll_tick_running = false;
                            false
                        } else {
                            true
                        }
                    }) else {
                        break;
                    };
                    if !keep_running {
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn set_viewport_origin(&mut self, pane_id: PaneId, origin: ViewPoint) -> bool {
        if self.viewport_origins.get(&pane_id) == Some(&origin) {
            return false;
        }
        self.viewport_origins.insert(pane_id, origin);
        true
    }

    fn set_pane_viewport_bounds(
        &mut self,
        pane_id: PaneId,
        viewport_width: f32,
        viewport_height: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> bool {
        let changed = self
            .panes
            .set_viewport_bounds(
                pane_id,
                viewport_width,
                viewport_height,
                max_scroll_x,
                max_scroll_y,
            )
            .unwrap_or(false);
        if changed {
            self.smooth_scrolls.remove(&pane_id);
            self.scroll_drag_trackers.remove(&pane_id);
        }
        changed
    }

    fn content_point_from_window(
        &self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<ViewPoint> {
        let origin = *self.viewport_origins.get(&pane_id)?;
        let view = &self.panes.pane(pane_id)?.view;
        Some(ViewPoint {
            x: (position.x.as_f32() - origin.x) + view.scroll_x,
            y: (position.y.as_f32() - origin.y) + view.scroll_y,
        })
    }

    fn layout_projection_for_pane(&mut self, pane_id: PaneId) -> Option<PaneLayoutProjection> {
        let filtered_model = self.filtered_model_for_pane(pane_id);
        let pane = self.panes.pane(pane_id)?;
        let layout = match filtered_model.as_ref() {
            Some((filtered, source_revision)) => compact_layout_for_filtered_model(
                self.compact_column_widths.entry(pane_id).or_default(),
                &pane.model,
                filtered,
                *source_revision,
                &pane.view,
            ),
            None => compact_layout_for_model(
                self.compact_column_widths.entry(pane_id).or_default(),
                &pane.model,
                &pane.view,
            ),
        };
        Some(PaneLayoutProjection {
            layout,
            filtered: filtered_model.map(|(filtered, _)| filtered),
        })
    }

    fn item_at_content_point(
        &mut self,
        pane_id: PaneId,
        point: ViewPoint,
    ) -> Option<ContentItemHit> {
        let projection = self.layout_projection_for_pane(pane_id)?;
        let layout_index = projection.layout.hit_test_content_point(point)?;
        let model_index = projection.model_index_for_layout_index(layout_index)?;
        let pane = self.panes.pane(pane_id)?;
        let entry = pane.model.get(model_index)?;
        let item_layout = projection.layout.item_with_required_text_width(
            layout_index,
            Some(compact_text_width(entry.name_width_units)),
        )?;
        if !item_layout.visual_rect.contains(point) {
            return None;
        }
        Some(ContentItemHit {
            model_index,
            path: pane.model.path_for_index(model_index)?,
            is_dir: entry.is_dir,
        })
    }

    fn indexes_intersecting_visual_rect(&mut self, pane_id: PaneId, rect: ViewRect) -> Vec<usize> {
        let Some(projection) = self.layout_projection_for_pane(pane_id) else {
            return Vec::new();
        };
        let candidate_indexes = projection
            .layout
            .indexes_intersecting(rect)
            .indexes()
            .to_vec();
        let Some(pane) = self.panes.pane(pane_id) else {
            return Vec::new();
        };
        candidate_indexes
            .into_iter()
            .filter_map(|layout_index| {
                let model_index = projection.model_index_for_layout_index(layout_index)?;
                let Some(entry) = pane.model.get(model_index) else {
                    return None;
                };
                projection
                    .layout
                    .item_with_required_text_width(
                        layout_index,
                        Some(compact_text_width(entry.name_width_units)),
                    )
                    .is_some_and(|item| item.visual_rect.intersects(rect))
                    .then_some(model_index)
            })
            .collect()
    }

    fn handle_blank_click(&mut self, pane_id: PaneId, position: gpui::Point<gpui::Pixels>) -> bool {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        let Some(point) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        if self.item_at_content_point(pane_id, point).is_some() {
            return false;
        }
        self.clear_selection_from_blank(pane_id);
        true
    }

    fn clear_selection_from_blank(&mut self, pane_id: PaneId) {
        self.clear_selection(pane_id);
    }

    fn start_rubber_band_from_blank(&mut self, pane_id: PaneId, start: ViewPoint) -> bool {
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        if self.item_at_content_point(pane_id, start).is_some() {
            return false;
        }
        self.start_rubber_band(pane_id, start);
        true
    }

    fn start_rubber_band(&mut self, pane_id: PaneId, start: ViewPoint) {
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.rubber_band = Some(RubberBandState {
            pane_id,
            start,
            current: start,
        });
    }

    fn update_rubber_band(&mut self, pane_id: PaneId, current: ViewPoint) {
        let Some(mut band) = self.rubber_band else {
            return;
        };
        if band.pane_id != pane_id {
            return;
        }
        band.current = current;
        self.rubber_band = Some(band);
        let selection = self.indexes_intersecting_visual_rect(pane_id, band.rect());
        if let Some(selected) = self
            .panes
            .replace_selection_by_indexes(pane_id, selection.iter().copied())
        {
            self.set_pane_status(pane_id, format!("{selected} selected"));
        }
    }

    fn finish_rubber_band(&mut self, pane_id: PaneId) {
        if self
            .rubber_band
            .as_ref()
            .is_some_and(|band| band.pane_id == pane_id)
        {
            self.rubber_band = None;
        }
    }

    fn clear_rename_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .rename_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.rename_draft = None;
        }
    }

    fn clear_location_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .location_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.location_draft = None;
        }
    }

    fn clear_place_draft_for_pane(&mut self, pane_id: PaneId) {
        if self
            .place_draft
            .as_ref()
            .is_some_and(|draft| draft.pane_id == pane_id)
        {
            self.place_draft = None;
        }
    }

    fn start_add_place(&mut self, pane_id: PaneId) {
        let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.panes.focus(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.place_draft = Some(PlaceDraft {
            pane_id,
            editing_path: None,
            focus: PlaceDraftField::Label,
            label: default_place_label(&path),
            path: path.display().to_string(),
        });
        self.set_pane_status(pane_id, format!("Adding place {}", path.display()));
    }

    fn start_edit_place(&mut self, pane_id: PaneId, path: PathBuf) {
        let Some(place) = self
            .places
            .iter()
            .find(|place| place.path == path && place.editable)
            .cloned()
        else {
            self.set_pane_status(pane_id, "Place cannot be edited");
            return;
        };
        self.panes.focus(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.place_draft = Some(PlaceDraft {
            pane_id,
            editing_path: Some(place.path.clone()),
            focus: PlaceDraftField::Label,
            label: place.label,
            path: place.path.display().to_string(),
        });
        self.set_pane_status(pane_id, "Editing place");
    }

    fn remove_place(&mut self, pane_id: PaneId, path: &Path) {
        let Some(index) = self
            .places
            .iter()
            .position(|place| place.path == path && place.removable)
        else {
            self.set_pane_status(pane_id, "Place cannot be removed");
            return;
        };
        let removed = self.places.remove(index);
        if self
            .place_draft
            .as_ref()
            .and_then(|draft| draft.editing_path.as_deref())
            == Some(removed.path.as_path())
        {
            self.place_draft = None;
        }
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, format!("Removed place {}", removed.label));
    }

    fn handle_place_draft_keystroke(&mut self, keystroke: &gpui::Keystroke) -> bool {
        let Some(draft_pane_id) = self.place_draft.as_ref().map(|draft| draft.pane_id) else {
            return false;
        };
        if self.panes.focused() != Some(draft_pane_id) {
            return false;
        }

        match place_input_action(keystroke) {
            PlaceInputAction::Cancel => {
                self.place_draft = None;
                self.set_pane_status(draft_pane_id, "Place edit cancelled");
            }
            PlaceInputAction::Commit => self.commit_place_draft(),
            PlaceInputAction::NextField => {
                if let Some(draft) = &mut self.place_draft {
                    draft.focus = match draft.focus {
                        PlaceDraftField::Label => PlaceDraftField::Path,
                        PlaceDraftField::Path => PlaceDraftField::Label,
                    };
                }
            }
            PlaceInputAction::Backspace => {
                if let Some(draft) = &mut self.place_draft {
                    match draft.focus {
                        PlaceDraftField::Label => {
                            draft.label.pop();
                        }
                        PlaceDraftField::Path => {
                            draft.path.pop();
                        }
                    }
                }
            }
            PlaceInputAction::Insert(text) => {
                if let Some(draft) = &mut self.place_draft {
                    match draft.focus {
                        PlaceDraftField::Label => draft.label.push_str(&text),
                        PlaceDraftField::Path => draft.path.push_str(&text),
                    }
                }
            }
            PlaceInputAction::Ignore => return false,
        }
        true
    }

    fn commit_place_draft(&mut self) {
        let Some(draft) = self.place_draft.take() else {
            return;
        };
        let label = draft.label.trim().to_string();
        if label.is_empty() {
            self.set_pane_status(draft.pane_id, "Place label cannot be empty");
            return;
        }
        let Some(current_dir) = self
            .panes
            .pane(draft.pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        let Some(path) = resolve_location_input(&current_dir, &draft.path) else {
            self.set_pane_status(draft.pane_id, "Place path cannot be empty");
            return;
        };
        if !path.is_dir() {
            self.set_pane_status(
                draft.pane_id,
                format!("Place path is not a folder: {}", path.display()),
            );
            return;
        }
        let duplicate = self.places.iter().position(|place| place.path == path);
        if let Some(editing_path) = draft.editing_path {
            let Some(index) = self
                .places
                .iter()
                .position(|place| place.path == editing_path && place.editable)
            else {
                self.set_pane_status(draft.pane_id, "Place cannot be edited");
                return;
            };
            if duplicate.is_some_and(|duplicate| duplicate != index) {
                self.set_pane_status(draft.pane_id, "Place already exists");
                return;
            }
            self.places[index].label = label.clone();
            self.places[index].path = path.clone();
            if let Err(error) = self.save_user_places() {
                self.set_pane_status(draft.pane_id, error);
                return;
            }
            self.set_pane_status(draft.pane_id, format!("Updated place {label}"));
            return;
        }

        if duplicate.is_some() {
            self.set_pane_status(draft.pane_id, "Place already exists");
            return;
        }
        self.insert_user_place(label.clone(), path);
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(draft.pane_id, error);
            return;
        }
        self.set_pane_status(draft.pane_id, format!("Added place {label}"));
    }

    fn insert_user_place(&mut self, label: String, path: PathBuf) {
        let entry = PlaceEntry {
            group: "",
            marker: "B",
            label,
            path,
            editable: true,
            removable: true,
        };
        let insert_at = self
            .places
            .iter()
            .position(|place| !place.group.is_empty())
            .unwrap_or(self.places.len());
        self.places.insert(insert_at, entry);
    }

    fn hide_place_section(&mut self, pane_id: PaneId, group: &'static str) {
        if group.is_empty() || !self.places.iter().any(|place| place.group == group) {
            self.set_pane_status(pane_id, "Place section cannot be hidden");
            return;
        }
        self.hidden_place_sections.insert(group);
        self.set_pane_status(pane_id, format!("Hidden places section {group}"));
    }

    fn show_hidden_places(&mut self, pane_id: PaneId) {
        if self.hidden_place_sections.is_empty() {
            self.set_pane_status(pane_id, "No hidden places");
            return;
        }
        self.hidden_place_sections.clear();
        self.set_pane_status(pane_id, "Showing hidden places");
    }

    fn user_places(&self) -> Vec<UserPlace> {
        self.places
            .iter()
            .filter(|place| place.editable && place.removable)
            .map(|place| UserPlace::new(place.label.clone(), place.path.clone()))
            .collect()
    }

    fn save_user_places(&self) -> Result<(), String> {
        fika_core::save_user_places(&self.user_places_path, &self.user_places())
    }

    pub(crate) fn start_location_edit(&mut self, pane_id: PaneId) {
        let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.panes.focus(pane_id);
        self.dismiss_context_menu();
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.location_draft = Some(LocationDraft {
            pane_id,
            value: path.display().to_string(),
        });
        self.set_pane_status(pane_id, format!("Location {}", path.display()));
    }

    pub(crate) fn open_location_segment(&mut self, pane_id: PaneId, path: PathBuf) {
        self.panes.focus(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        if self
            .panes
            .pane(pane_id)
            .is_some_and(|pane| pane.current_dir == path)
        {
            return;
        }
        self.load_pane(pane_id, path);
    }

    fn handle_location_keystroke(&mut self, keystroke: &gpui::Keystroke) -> bool {
        let Some(draft_pane_id) = self.location_draft.as_ref().map(|draft| draft.pane_id) else {
            return false;
        };
        if self.panes.focused() != Some(draft_pane_id) {
            return false;
        }

        match location_input_action(keystroke) {
            LocationInputAction::Cancel => {
                self.location_draft = None;
                self.set_pane_status(draft_pane_id, "Location edit cancelled");
            }
            LocationInputAction::Commit => self.commit_location_draft(),
            LocationInputAction::Complete => self.complete_location_draft(),
            LocationInputAction::Backspace => {
                if let Some(draft) = &mut self.location_draft {
                    draft.value.pop();
                }
            }
            LocationInputAction::Insert(text) => {
                if let Some(draft) = &mut self.location_draft {
                    draft.value.push_str(&text);
                }
            }
            LocationInputAction::Ignore => return false,
        }
        true
    }

    fn commit_location_draft(&mut self) {
        let Some(draft) = self.location_draft.take() else {
            return;
        };
        let Some(current_dir) = self
            .panes
            .pane(draft.pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        let Some(path) = resolve_location_input(&current_dir, &draft.value) else {
            self.set_pane_status(draft.pane_id, "Location is empty");
            return;
        };
        if !path.is_dir() {
            self.set_pane_status(
                draft.pane_id,
                format!("Location is not a folder: {}", path.display()),
            );
            return;
        }
        if path == current_dir {
            self.set_pane_status(draft.pane_id, format!("Location {}", path.display()));
            return;
        }
        self.load_pane(draft.pane_id, path);
    }

    fn complete_location_draft(&mut self) {
        let Some(draft) = self.location_draft.clone() else {
            return;
        };
        let Some(current_dir) = self
            .panes
            .pane(draft.pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        let Some(completed) = complete_location_input(&current_dir, &draft.value) else {
            self.set_pane_status(draft.pane_id, "No location completion");
            return;
        };
        if let Some(active) = &mut self.location_draft {
            active.value = completed;
        }
    }

    fn start_rename_in_pane(&mut self, pane_id: PaneId) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        let [original_path] = selected_paths.as_slice() else {
            self.set_pane_status(pane_id, "Select one item to rename");
            return;
        };
        let Some(name) = original_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
        else {
            self.set_pane_status(pane_id, "Selected item cannot be renamed");
            return;
        };

        self.clear_location_draft_for_pane(pane_id);
        self.clear_place_draft_for_pane(pane_id);
        self.rename_draft = Some(RenameDraft {
            pane_id,
            original_path: original_path.clone(),
            draft_name: name.to_string(),
        });
        self.set_pane_status(pane_id, format!("Renaming {name}"));
    }

    fn handle_rename_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(draft_pane_id) = self.rename_draft.as_ref().map(|draft| draft.pane_id) else {
            return false;
        };
        if self.panes.focused() != Some(draft_pane_id) {
            return false;
        }

        match rename_input_action(keystroke) {
            RenameInputAction::Cancel => {
                self.rename_draft = None;
                self.set_pane_status(draft_pane_id, "Rename cancelled");
            }
            RenameInputAction::Commit => self.commit_rename_draft(cx),
            RenameInputAction::Backspace => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.draft_name.pop();
                }
            }
            RenameInputAction::Insert(text) => {
                if let Some(draft) = &mut self.rename_draft {
                    draft.draft_name.push_str(&text);
                }
            }
            RenameInputAction::Ignore => {}
        }
        true
    }

    fn commit_rename_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft_pane_id) = self.rename_draft.as_ref().map(|draft| draft.pane_id) else {
            return;
        };
        if self.operation_pending {
            self.set_pane_status(draft_pane_id, "File operation already running");
            return;
        }
        let Some(draft) = self.rename_draft.take() else {
            return;
        };
        let new_name = draft.draft_name.trim().to_string();
        if new_name.is_empty() {
            self.set_pane_status(draft.pane_id, "Name cannot be empty");
            return;
        }
        if draft
            .original_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == new_name)
        {
            let _ = self
                .panes
                .select_only(draft.pane_id, draft.original_path.clone());
            self.set_pane_status(draft.pane_id, "Rename unchanged");
            return;
        }

        self.begin_pane_operation(
            draft.pane_id,
            format!("Renaming {}", draft.original_path.display()),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            rename_item_result(draft.pane_id, draft.original_path, new_name)
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_rename_item(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_rename_item(&mut self, result: RenameItemResult) {
        match result.result {
            Ok(renamed_path) => {
                self.operations.register_undo_with_payload(
                    "Rename".to_string(),
                    result.affected_dirs.clone(),
                    UndoPayload::Rename {
                        items: vec![RenameUndoItem {
                            original_path: result.original_path.clone(),
                            renamed_path: renamed_path.clone(),
                        }],
                    },
                );
                self.refresh_affected_dirs(&result.affected_dirs);
                let _ = self.panes.select_only(result.pane_id, renamed_path.clone());
                self.finish_pane_operation(
                    result.pane_id,
                    format!("Renamed to {}", renamed_path.display()),
                );
            }
            Err(err) => {
                self.finish_pane_operation(
                    result.pane_id,
                    format!("Cannot rename {}: {err}", result.original_path.display()),
                );
            }
        }
    }

    fn create_item_in_pane(
        &mut self,
        pane_id: PaneId,
        kind: CreatedItemKind,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let Some(parent_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };

        self.begin_pane_operation(
            pane_id,
            format!("Creating {}", created_item_label(kind).to_ascii_lowercase()),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(
                            async move { create_item_result(pane_id, parent_dir, kind) },
                        )
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_create_item(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_create_item(&mut self, result: CreateItemResult) {
        match result.result {
            Ok(path) => {
                self.operations.register_undo_with_payload(
                    format!("Create {}", created_item_label(result.kind)),
                    result.affected_dirs.clone(),
                    UndoPayload::Create {
                        items: vec![CreateUndoItem {
                            path: path.clone(),
                            kind: result.kind,
                        }],
                    },
                );
                self.refresh_affected_dirs(&result.affected_dirs);
                let _ = self.panes.select_only(result.pane_id, path.clone());
                self.finish_pane_operation(result.pane_id, format!("Created {}", path.display()));
            }
            Err(err) => {
                self.finish_pane_operation(
                    result.pane_id,
                    format!(
                        "Cannot create {}: {err}",
                        created_item_label(result.kind).to_ascii_lowercase()
                    ),
                );
            }
        }
    }

    fn store_selection_for_transfer(
        &mut self,
        pane_id: PaneId,
        mode: ClipboardMode,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        let paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if paths.is_empty() {
            self.set_pane_status(
                pane_id,
                format!("No selection to {}", mode.label().to_ascii_lowercase()),
            );
            return;
        }

        let count = paths.len();
        let clipboard = ClipboardState::files(mode, paths);
        let item = clipboard.to_clipboard_item();
        cx.write_to_clipboard(item.clone());
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        cx.write_to_primary(item);
        self.clipboard = Some(clipboard);
        self.set_pane_status(pane_id, format!("{} {} item(s)", mode.label(), count));
    }

    fn import_system_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(clipboard) = cx
            .read_from_clipboard()
            .as_ref()
            .and_then(ClipboardState::from_clipboard_item)
        {
            self.clipboard = Some(clipboard);
            return;
        }

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if let Some(clipboard) = cx
            .read_from_primary()
            .as_ref()
            .and_then(ClipboardState::from_clipboard_item)
        {
            self.clipboard = Some(clipboard);
        }
    }

    fn paste_into_pane(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        let Some(target_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.paste_into_directory(pane_id, target_dir, cx);
    }

    fn paste_into_directory(
        &mut self,
        pane_id: PaneId,
        target_dir: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        self.import_system_clipboard(cx);
        let Some(clipboard) = self.clipboard.clone() else {
            self.set_pane_status(pane_id, "Nothing to paste");
            return;
        };
        if !target_dir.is_dir() {
            self.set_pane_status(
                pane_id,
                format!("Cannot paste into {}", target_dir.display()),
            );
            return;
        }

        let progress_label = clipboard.progress_label();
        self.begin_pane_operation(pane_id, progress_label.clone());
        let (cancel, progress) = self.start_transfer_progress(pane_id, progress_label);
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            paste_clipboard_result(
                                pane_id,
                                target_dir,
                                clipboard,
                                Some(cancel),
                                Some(progress),
                            )
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_paste(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_paste(&mut self, result: PasteTaskResult) {
        self.clear_operation_progress();
        let PasteTaskResult {
            pane_id,
            mode,
            label,
            success_count,
            failure_count,
            affected_dirs,
            undo_items,
            created_items,
        } = result;

        if success_count > 0 {
            let created_selection = created_items.first().map(|item| item.path.clone());
            let has_transfer_items = !undo_items.is_empty();
            if has_transfer_items {
                self.operations.register_undo_with_payload(
                    mode.label().to_string(),
                    affected_dirs.clone(),
                    UndoPayload::Transfer { items: undo_items },
                );
            }
            if !created_items.is_empty() {
                self.operations.register_undo_with_payload(
                    label.to_string(),
                    affected_dirs.clone(),
                    UndoPayload::Create {
                        items: created_items,
                    },
                );
            }
            self.refresh_affected_dirs(&affected_dirs);
            if let Some(path) = created_selection {
                let _ = self.panes.select_only(pane_id, path);
            }
            if mode == ClipboardMode::Cut && has_transfer_items {
                self.clipboard = None;
                let _ = self.panes.clear_selection(pane_id);
            }
        }

        self.finish_pane_operation(
            pane_id,
            action_status(&format!("{label} complete"), success_count, failure_count),
        );
    }

    fn trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.set_pane_status(pane_id, "No selection to trash");
            return;
        }

        self.begin_pane_operation(
            pane_id,
            format!("Moving {} item(s) to trash", selected_paths.len()),
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(
                            async move { trash_selection_result(pane_id, selected_paths) },
                        )
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_trash_selection(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_trash_selection(&mut self, result: TrashSelectionResult) {
        if result.success_count > 0 {
            self.operations.register_undo_with_payload(
                "Move to Trash".to_string(),
                result.affected_dirs.clone(),
                UndoPayload::Trash {
                    items: result.undo_items,
                },
            );
            self.refresh_affected_dirs(&result.affected_dirs);
            let _ = self.panes.clear_selection(result.pane_id);
        }

        self.finish_pane_operation(
            result.pane_id,
            action_status("Moved to trash", result.success_count, result.failure_count),
        );
    }

    fn restore_trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        self.start_trash_view_selection_operation(
            pane_id,
            TrashViewOperation::Restore,
            "No trash selection to restore",
            cx,
        );
    }

    fn delete_trash_selection_permanently(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        self.start_trash_view_selection_operation(
            pane_id,
            TrashViewOperation::DeletePermanently,
            "No trash selection to delete",
            cx,
        );
    }

    fn start_trash_view_selection_operation(
        &mut self,
        pane_id: PaneId,
        operation: TrashViewOperation,
        empty_selection_status: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        if !self.trash_view_state(pane_id).0 {
            self.set_pane_status(pane_id, "Trash action is only available in Trash");
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.set_pane_status(pane_id, empty_selection_status);
            return;
        }
        self.start_trash_view_operation(pane_id, operation, selected_paths, cx);
    }

    fn empty_trash(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let (trash_view, trash_has_items) = self.trash_view_state(pane_id);
        if !trash_view {
            self.set_pane_status(pane_id, "Empty Trash is only available in Trash");
            return;
        }
        if !trash_has_items {
            self.set_pane_status(pane_id, "Trash is empty");
            return;
        }
        self.start_trash_view_operation(pane_id, TrashViewOperation::Empty, Vec::new(), cx);
    }

    fn empty_trash_from_place(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        if !file_ops::trash_has_items() {
            self.set_pane_status(pane_id, "Trash is empty");
            return;
        }
        self.start_trash_view_operation(pane_id, TrashViewOperation::Empty, Vec::new(), cx);
    }

    fn start_trash_view_operation(
        &mut self,
        pane_id: PaneId,
        operation: TrashViewOperation,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.begin_pane_operation(pane_id, operation.progress_label(paths.len()));
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            trash_view_operation_result(pane_id, operation, paths)
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_trash_view_operation(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_trash_view_operation(&mut self, result: TrashViewOperationResult) {
        if result.success_count > 0 {
            self.refresh_affected_dirs(&result.affected_dirs);
            let _ = self.panes.clear_selection(result.pane_id);
        }
        self.finish_pane_operation(
            result.pane_id,
            action_status(
                result.operation.completed_label(),
                result.success_count,
                result.failure_count,
            ),
        );
    }

    fn undo_latest(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.set_pane_status(pane_id, "File operation already running");
            return;
        }
        let Some(record) = self.operations.latest_undo().cloned() else {
            self.set_pane_status(pane_id, "No operation to undo");
            return;
        };

        match &record.payload {
            UndoPayload::Create { .. } => {}
            UndoPayload::Rename { .. } => {}
            UndoPayload::Trash { .. } => {}
            UndoPayload::Transfer { .. } => {}
            UndoPayload::None => {
                self.set_pane_status(pane_id, format!("No undo action for {}", record.label));
                return;
            }
        }

        self.begin_pane_operation(pane_id, format!("Undoing {}", record.label));
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move { undo_record_result(record) })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_undo(pane_id, result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_undo(&mut self, pane_id: PaneId, result: UndoTaskResult) {
        match result.result {
            Ok(message) => {
                if self
                    .operations
                    .take_latest_undo(result.record.serial)
                    .is_none()
                {
                    self.finish_pane_operation(pane_id, "Undo result is stale");
                    return;
                }
                self.refresh_affected_dirs(&result.record.affected_dirs);
                self.finish_pane_operation(
                    pane_id,
                    format!("Undid {}: {message}", result.record.label),
                );
            }
            Err(err) => {
                self.finish_pane_operation(
                    pane_id,
                    format!("Cannot undo {}: {err}", result.record.label),
                );
            }
        }
    }

    fn refresh_affected_dirs(&mut self, affected_dirs: &[PathBuf]) {
        let refreshes = OperationQueue::refresh_affected_panes(&mut self.panes, affected_dirs);
        self.schedule_listings(refreshes.iter().map(|refresh| &refresh.event));
        for refresh in refreshes {
            self.apply_event(refresh.event);
            self.start_watcher(refresh.pane_id);
        }
    }

    fn show_blank_context_menu_if_blank(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        self.panes.focus(pane_id);
        self.finish_rubber_band(pane_id);
        let Some(point) = self.content_point_from_window(pane_id, position) else {
            return false;
        };
        if self.item_at_content_point(pane_id, point).is_some() {
            return false;
        }
        self.show_blank_context_menu(
            pane_id,
            ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
        );
        true
    }

    fn show_blank_context_menu(&mut self, pane_id: PaneId, position: ViewPoint) {
        let (trash_view, trash_has_items) = self.trash_view_state(pane_id);
        self.context_menu = Some(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Blank {
                trash_view,
                trash_has_items,
            },
            position,
            active_submenu: None,
        });
    }

    fn show_item_context_menu(
        &mut self,
        pane_id: PaneId,
        path: PathBuf,
        is_dir: bool,
        position: gpui::Point<gpui::Pixels>,
    ) {
        self.panes.focus(pane_id);
        self.finish_rubber_band(pane_id);
        if !self.panes.is_selected(pane_id, &path) {
            self.select_only(pane_id, path.clone());
        }
        let selection_count = self.panes.selected_count(pane_id).unwrap_or(1).max(1);
        let trash_view = self.trash_view_state(pane_id).0;
        let trash_can_restore = trash_view && file_ops::trash_metadata(&path).is_ok();
        let menu_position = ViewPoint {
            x: position.x.as_f32(),
            y: position.y.as_f32(),
        };
        self.context_menu = Some(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Item {
                path,
                is_dir,
                selection_count,
                trash_view,
                trash_can_restore,
            },
            position: menu_position,
            active_submenu: None,
        });
    }

    fn trash_view_state(&self, pane_id: PaneId) -> (bool, bool) {
        self.panes
            .pane(pane_id)
            .map(|pane| {
                let trash_view = file_ops::is_trash_files_dir(&pane.current_dir);
                let trash_has_items = trash_view && !pane.model.is_empty();
                (trash_view, trash_has_items)
            })
            .unwrap_or_default()
    }

    fn dismiss_context_menu(&mut self) {
        self.context_menu = None;
    }

    fn open_context_submenu(&mut self, submenu: ContextMenuSubmenu, parent_index: usize) {
        if let Some(menu) = self.context_menu.as_mut() {
            menu.active_submenu = Some(ContextMenuOpenSubmenu {
                submenu,
                parent_index,
            });
        }
    }

    fn clear_context_submenu(&mut self) {
        if let Some(menu) = self.context_menu.as_mut() {
            menu.active_submenu = None;
        }
    }

    fn dismiss_place_draft(&mut self) {
        self.place_draft = None;
    }

    fn set_place_draft_focus(&mut self, field: PlaceDraftField) {
        if let Some(draft) = &mut self.place_draft {
            draft.focus = field;
        }
    }

    fn dismiss_properties_dialog(&mut self) {
        self.properties_dialog = None;
    }

    fn show_properties_for_context(&mut self, pane_id: PaneId, target: ContextMenuTarget) {
        let dialog = match target {
            ContextMenuTarget::Blank { .. } => {
                let Some(path) = self
                    .panes
                    .pane(pane_id)
                    .map(|pane| pane.current_dir.clone())
                else {
                    return;
                };
                properties_for_path(&path)
            }
            ContextMenuTarget::Item {
                path,
                selection_count,
                ..
            } if selection_count > 1 => {
                let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
                if selected_paths.is_empty() {
                    properties_for_path(&path)
                } else {
                    properties_for_selection(&selected_paths)
                }
            }
            ContextMenuTarget::Item { path, .. } => properties_for_path(&path),
            ContextMenuTarget::Place { path, .. } => properties_for_path(&path),
            ContextMenuTarget::PlacesBlank { .. } | ContextMenuTarget::PlaceSection { .. } => {
                return;
            }
        };
        self.properties_dialog = Some(dialog);
    }

    fn run_context_menu_action(&mut self, action: ContextMenuAction, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.clone() else {
            return;
        };
        self.dismiss_context_menu();
        self.panes.focus(menu.pane_id);

        match (action, menu.target) {
            (
                ContextMenuAction::Open,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.load_pane(menu.pane_id, path),
            (
                ContextMenuAction::OpenInNewPane,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.open_path_in_new_pane(menu.pane_id, path),
            (
                ContextMenuAction::Open,
                ContextMenuTarget::Item {
                    path,
                    is_dir: false,
                    ..
                },
            ) => {
                self.select_only(menu.pane_id, path.clone());
                self.set_pane_status(
                    menu.pane_id,
                    format!("Open with is not implemented for {}", path.display()),
                );
            }
            (ContextMenuAction::Open, ContextMenuTarget::Place { path, .. }) => {
                self.open_place(path);
            }
            (ContextMenuAction::OpenInNewPane, ContextMenuTarget::Place { path, .. }) => {
                self.open_path_in_new_pane(menu.pane_id, path);
            }
            (ContextMenuAction::AddPlace, ContextMenuTarget::PlacesBlank { .. }) => {
                self.start_add_place(menu.pane_id);
            }
            (
                ContextMenuAction::EditPlace,
                ContextMenuTarget::Place {
                    path,
                    editable: true,
                    ..
                },
            ) => self.start_edit_place(menu.pane_id, path),
            (
                ContextMenuAction::RemovePlace,
                ContextMenuTarget::Place {
                    path,
                    removable: true,
                    ..
                },
            ) => self.remove_place(menu.pane_id, &path),
            (ContextMenuAction::HidePlaceSection, ContextMenuTarget::PlaceSection { group }) => {
                self.hide_place_section(menu.pane_id, group);
            }
            (ContextMenuAction::ShowHiddenPlaces, ContextMenuTarget::PlacesBlank { .. }) => {
                self.show_hidden_places(menu.pane_id);
            }
            (ContextMenuAction::Rename, ContextMenuTarget::Item { path, .. }) => {
                self.select_only(menu.pane_id, path);
                self.start_rename_in_pane(menu.pane_id);
            }
            (ContextMenuAction::Copy, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::Blank { .. }) => {
                self.store_selection_for_transfer(menu.pane_id, ClipboardMode::Copy, cx)
            }
            (ContextMenuAction::CopyLocation, ContextMenuTarget::Item { path, .. }) => {
                let location = path.display().to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(location));
                self.set_pane_status(menu.pane_id, format!("Copied location {}", path.display()));
            }
            (ContextMenuAction::CopyLocation, ContextMenuTarget::Place { path, .. }) => {
                let location = path.display().to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(location));
                self.set_pane_status(menu.pane_id, format!("Copied location {}", path.display()));
            }
            (ContextMenuAction::Cut, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::Blank { .. }) => {
                self.store_selection_for_transfer(menu.pane_id, ClipboardMode::Cut, cx)
            }
            (ContextMenuAction::Trash, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::Blank { .. }) => {
                self.trash_selection(menu.pane_id, cx)
            }
            (ContextMenuAction::RestoreFromTrash, ContextMenuTarget::Item { .. }) => {
                self.restore_trash_selection(menu.pane_id, cx)
            }
            (ContextMenuAction::DeletePermanently, ContextMenuTarget::Item { .. }) => {
                self.delete_trash_selection_permanently(menu.pane_id, cx)
            }
            (ContextMenuAction::EmptyTrash, ContextMenuTarget::Blank { .. }) => {
                self.empty_trash(menu.pane_id, cx)
            }
            (
                ContextMenuAction::EmptyTrash,
                ContextMenuTarget::Place {
                    trash_place: true, ..
                },
            ) => self.empty_trash_from_place(menu.pane_id, cx),
            (ContextMenuAction::Properties, target) => {
                self.show_properties_for_context(menu.pane_id, target)
            }
            (ContextMenuAction::CreateFolder, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Paste, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::SelectAll, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Refresh, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Properties, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Properties, ContextMenuTarget::PlaceSection { .. }) => {}
            (ContextMenuAction::CreateFolder, _) => {
                self.create_item_in_pane(menu.pane_id, CreatedItemKind::Folder, cx)
            }
            (
                ContextMenuAction::Paste,
                ContextMenuTarget::Item {
                    path, is_dir: true, ..
                },
            ) => self.paste_into_directory(menu.pane_id, path, cx),
            (ContextMenuAction::Paste, _) => self.paste_into_pane(menu.pane_id, cx),
            (ContextMenuAction::SelectAll, _) => self.select_all(menu.pane_id),
            (ContextMenuAction::Refresh, _) => self.reload_pane(menu.pane_id),
            (ContextMenuAction::ViewCompact, _) => {
                self.set_pane_status(menu.pane_id, "Compact view")
            }
            (ContextMenuAction::SortByName, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::Name)
            }
            (ContextMenuAction::SortByModified, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::Modified)
            }
            (ContextMenuAction::SortBySize, _) => {
                self.set_pane_sort_role(menu.pane_id, SortRole::Size)
            }
            (ContextMenuAction::SortAscending, _) => {
                self.set_pane_sort_order(menu.pane_id, SortOrder::Ascending)
            }
            (ContextMenuAction::SortDescending, _) => {
                self.set_pane_sort_order(menu.pane_id, SortOrder::Descending)
            }
            (ContextMenuAction::SortFoldersFirst, _) => {
                let folders_first = self
                    .panes
                    .sort_descriptor(menu.pane_id)
                    .map(|sort| !sort.folders_first)
                    .unwrap_or(true);
                self.set_pane_sort_folders_first(menu.pane_id, folders_first);
            }
            (ContextMenuAction::SortHiddenLast, _) => {
                let hidden_last = self
                    .panes
                    .sort_descriptor(menu.pane_id)
                    .map(|sort| !sort.hidden_last)
                    .unwrap_or(false);
                self.set_pane_sort_hidden_last(menu.pane_id, hidden_last);
            }
            (
                ContextMenuAction::SortBySubmenu
                | ContextMenuAction::ViewModeSubmenu
                | ContextMenuAction::ViewIcons
                | ContextMenuAction::ViewDetails,
                _,
            ) => {}
            (ContextMenuAction::Open, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Open, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::OpenInNewPane, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::PlacesBlank { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::Open, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::OpenInNewPane, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::PlaceSection { .. })
            | (ContextMenuAction::OpenInNewPane, _)
            | (ContextMenuAction::AddPlace, _)
            | (ContextMenuAction::EditPlace, _)
            | (ContextMenuAction::RemovePlace, _)
            | (ContextMenuAction::HidePlaceSection, _)
            | (ContextMenuAction::ShowHiddenPlaces, _)
            | (ContextMenuAction::Rename, ContextMenuTarget::Blank { .. })
            | (ContextMenuAction::Rename, ContextMenuTarget::Place { .. })
            | (ContextMenuAction::RestoreFromTrash, _)
            | (ContextMenuAction::DeletePermanently, _)
            | (ContextMenuAction::EmptyTrash, _) => {}
        }
    }

    fn handle_keystroke(&mut self, event: &gpui::KeystrokeEvent, cx: &mut Context<Self>) -> bool {
        if event.keystroke.key.eq_ignore_ascii_case("escape") && self.properties_dialog.is_some() {
            self.dismiss_properties_dialog();
            return true;
        }
        if event.keystroke.key.eq_ignore_ascii_case("escape") && self.context_menu.is_some() {
            self.dismiss_context_menu();
            return true;
        }
        if self.handle_location_keystroke(&event.keystroke) {
            return true;
        }
        if self.handle_rename_keystroke(&event.keystroke, cx) {
            return true;
        }
        if self.handle_place_draft_keystroke(&event.keystroke) {
            return true;
        }
        let Some(pane_id) = self.panes.focused() else {
            return false;
        };
        if self.handle_filter_keystroke(pane_id, &event.keystroke) {
            return true;
        }
        match pane_shortcut(&event.keystroke) {
            Some(PaneShortcut::SelectAll) => self.select_all(pane_id),
            Some(PaneShortcut::ClearSelection) => self.clear_selection(pane_id),
            Some(PaneShortcut::Refresh) => self.reload_pane(pane_id),
            Some(PaneShortcut::GoParent) => self.go_parent(pane_id),
            Some(PaneShortcut::GoBack) => self.go_back(pane_id),
            Some(PaneShortcut::GoForward) => self.go_forward(pane_id),
            Some(PaneShortcut::SplitPane) => self.split_pane(pane_id),
            Some(PaneShortcut::ClosePane) => self.close_pane(pane_id),
            Some(PaneShortcut::EditLocation) => self.start_location_edit(pane_id),
            Some(PaneShortcut::ShowFilter) => self.show_filter_bar(pane_id),
            Some(PaneShortcut::Zoom(change)) => self.apply_zoom_change(pane_id, change),
            Some(PaneShortcut::MoveSelection { direction, extend }) => {
                self.move_selection(pane_id, direction, extend)
            }
            Some(PaneShortcut::CreateFolder) => {
                self.create_item_in_pane(pane_id, CreatedItemKind::Folder, cx)
            }
            Some(PaneShortcut::RenameSelection) => self.start_rename_in_pane(pane_id),
            Some(PaneShortcut::CopySelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Copy, cx)
            }
            Some(PaneShortcut::CutSelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Cut, cx)
            }
            Some(PaneShortcut::PasteIntoPane) => self.paste_into_pane(pane_id, cx),
            Some(PaneShortcut::TrashSelection) => self.trash_selection(pane_id, cx),
            Some(PaneShortcut::Undo) => self.undo_latest(pane_id, cx),
            None => return false,
        }
        true
    }

    fn confirm_chooser(&mut self) {
        if self.chooser.is_none() {
            return;
        }
        let selected_paths = self
            .panes
            .focused()
            .and_then(|pane_id| self.panes.selected_paths(pane_id))
            .unwrap_or_default();
        if selected_paths.is_empty() {
            if self
                .chooser
                .as_ref()
                .is_some_and(|chooser| chooser.directories)
            {
                if let Some(path) = self
                    .panes
                    .focused()
                    .and_then(|pane_id| self.panes.pane(pane_id))
                    .map(|pane| pane.current_dir.clone())
                {
                    self.choose_path(path);
                    return;
                }
            }
            if let Some(pane_id) = self.panes.focused() {
                self.set_pane_status(pane_id, "No chooser selection");
            }
            return;
        }
        self.choose_paths(selected_paths);
    }

    fn choose_path(&mut self, path: PathBuf) {
        self.choose_paths(vec![path]);
    }

    fn choose_paths(&mut self, paths: Vec<PathBuf>) {
        if let Some(chooser) = &self.chooser {
            if chooser.return_filter {
                println!("FIKA_CHOOSER_FILTER\t{}", chooser.filter_index);
            }
            if chooser.return_choices {
                for choice in selected_choice_rows(&chooser.choices) {
                    println!("{choice}");
                }
            }
        }
        for path in paths {
            println!("{}", path.display());
        }
        std::process::exit(0);
    }

    fn apply_event(&mut self, event: DirectoryListerEvent) {
        self.update_loading_state(&event);
        if let DirectoryListerEvent::CurrentDirectoryRemoved { pane_id, path, .. } = &event {
            self.listing_worker.remove_cached_directory(path);
            let still_current = self.panes.pane(*pane_id).is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            });
            if still_current {
                let fallback =
                    nearest_existing_ancestor(path).unwrap_or_else(|| PathBuf::from("/"));
                self.set_pane_status(*pane_id, format!("{} was removed", path.display()));
                self.load_pane(*pane_id, fallback);
            }
            return;
        }

        match &event {
            DirectoryListerEvent::ItemsAdded { path, .. }
            | DirectoryListerEvent::ItemsDeleted { path, .. }
            | DirectoryListerEvent::ItemsRefreshed { path, .. } => {
                self.listing_worker.mark_cache_stale(path);
            }
            _ => {}
        }

        let pane_id = event.pane_id();
        if let Some(signals) = self.panes.apply_lister_event(event) {
            if !signals.is_empty() {
                self.invalidate_pane_layout_projection(pane_id, false);
                self.set_pane_status(pane_id, format!("{} model signal(s)", signals.len()));
            }
        }
    }

    fn update_loading_state(&mut self, event: &DirectoryListerEvent) {
        update_loading_state_for_event(
            &mut self.loading_panes,
            self.panes.pane(event.pane_id()),
            event,
            Instant::now(),
        );
    }

    fn start_watchers(&mut self) {
        for pane_id in self.panes.pane_ids().to_vec() {
            self.start_watcher(pane_id);
        }
    }

    fn start_watcher(&mut self, pane_id: PaneId) {
        let Some(pane) = self.panes.pane_mut(pane_id) else {
            return;
        };
        let current_dir = pane.current_dir.clone();
        if let Err(err) = pane.lister.start_watcher() {
            self.set_pane_status(
                pane_id,
                format!("Cannot watch {}: {err}", current_dir.display()),
            );
        }
    }

    fn schedule_listing(&self, event: &DirectoryListerEvent) -> Option<Vec<DirectoryListerEvent>> {
        let request = ListingRequest::from_event(event)?;
        self.listing_worker.schedule_or_cached(request)
    }

    fn schedule_listings<'a>(&self, events: impl IntoIterator<Item = &'a DirectoryListerEvent>) {
        self.listing_worker
            .schedule_all(listing_requests_from_events(events));
    }

    fn apply_cached_listing_events(&mut self, events: Option<Vec<DirectoryListerEvent>>) {
        for event in events.unwrap_or_default() {
            self.apply_event(event);
        }
    }

    fn drain_background_listing_results(&mut self) -> bool {
        let mut changed = false;
        for events in self.listing_worker.drain_results() {
            for event in events {
                self.apply_event(event);
                changed = true;
            }
        }
        changed
    }

    fn drain_watchers(&mut self) -> bool {
        let mut changed = false;
        let pane_ids = self.panes.pane_ids().to_vec();
        let mut events = Vec::new();
        for pane_id in pane_ids {
            events.extend(
                self.panes
                    .pane_mut(pane_id)
                    .map(|pane| pane.lister.drain_watcher_events())
                    .unwrap_or_default(),
            );
        }
        self.schedule_listings(events.iter());
        for event in events {
            self.apply_event(event);
            changed = true;
        }
        changed
    }
}

impl Render for FikaApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .chooser
            .as_ref()
            .map(|chooser| chooser.title.as_str())
            .unwrap_or("Fika");
        window.set_window_title(title);
        let places = self.place_snapshots();
        let snapshots = self.snapshots(cx);
        let file_grid_mode =
            self.chooser
                .as_ref()
                .map_or(ui::file_grid::FileGridMode::Manager, |chooser| {
                    ui::file_grid::FileGridMode::Chooser {
                        directories: chooser.directories,
                        multiple: chooser.multiple,
                    }
                });
        let chooser_action_label = self.chooser.as_ref().map(|chooser| {
            let target = if chooser.directories {
                "folders"
            } else {
                "files"
            };
            let count = if chooser.multiple {
                "multiple"
            } else {
                "single"
            };
            format!("{} - {} {}", chooser.accept_label, count, target)
        });
        let pane_elements = snapshots
            .into_iter()
            .map(|snapshot| {
                ui::pane::pane_view(
                    ui::pane::PaneProps {
                        snapshot,
                        file_grid_mode,
                    },
                    cx,
                )
            })
            .collect::<Vec<_>>();
        let context_menu = self.context_menu.clone();
        let properties_dialog = self.properties_dialog.clone();
        let place_draft = self.place_draft.clone();
        let clipboard_available = self.clipboard.is_some();
        let viewport_size = window.viewport_size();
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xf0f2f5))
            .text_color(rgb(0x1f2328))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0xc8ced6))
                    .bg(rgb(0xffffff))
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(
                        if self.chooser.is_some() {
                            "Fika Chooser"
                        } else {
                            "Fika"
                        },
                    ))
                    .child(
                        div().text_sm().text_color(rgb(0x59636e)).child(
                            chooser_action_label
                                .clone()
                                .unwrap_or_else(|| "GPUI directory shell".to_string()),
                        ),
                    )
                    .when(self.chooser.is_some(), |bar| {
                        bar.child(ui::controls::toolbar_button("choose", "Choose").on_click(
                            cx.listener(move |this, _event, _window, cx| {
                                this.confirm_chooser();
                                cx.notify();
                            }),
                        ))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(ui::places::places_sidebar(places, cx))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .p_2()
                            .flex_1()
                            .children(pane_elements),
                    ),
            )
            .when_some(context_menu, |root, menu| {
                root.child(context_menu_overlay(
                    menu,
                    clipboard_available,
                    viewport_size.width.as_f32(),
                    viewport_size.height.as_f32(),
                    cx,
                ))
            })
            .when_some(properties_dialog, |root, dialog| {
                root.child(properties_dialog_overlay(dialog, cx))
            })
            .when_some(place_draft, |root, draft| {
                root.child(place_draft_overlay(draft, cx))
            })
    }
}

fn context_menu_overlay(
    menu: ContextMenuState,
    clipboard_available: bool,
    viewport_width: f32,
    viewport_height: f32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let actions = context_menu_actions(&menu.target, clipboard_available);
    let submenu = menu.active_submenu.map(|open| {
        let actions = context_submenu_actions(open.submenu);
        let submenu_height =
            CONTEXT_MENU_VERTICAL_PADDING * 2.0 + actions.len() as f32 * CONTEXT_MENU_ROW_HEIGHT;
        let opens_right = menu.position.x + CONTEXT_MENU_WIDTH * 2.0 <= viewport_width;
        let x = if opens_right {
            menu.position.x + CONTEXT_MENU_WIDTH - 1.0
        } else {
            (menu.position.x - CONTEXT_MENU_WIDTH + 1.0).max(0.0)
        };
        let y = (menu.position.y
            + CONTEXT_MENU_VERTICAL_PADDING
            + open.parent_index as f32 * CONTEXT_MENU_ROW_HEIGHT)
            .min((viewport_height - submenu_height).max(0.0));
        (open, actions, x, y)
    });
    div()
        .id("context-menu-layer")
        .absolute()
        .inset_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_context_menu();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_context_menu();
                cx.notify();
            }),
        )
        .child(
            div()
                .id(format!("context-menu-{}", menu.pane_id.0))
                .absolute()
                .left(px(menu.position.x))
                .top(px(menu.position.y))
                .w(px(CONTEXT_MENU_WIDTH))
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xc8ced6))
                .bg(rgb(0xffffff))
                .shadow_md()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .children(
                    actions
                        .into_iter()
                        .enumerate()
                        .map(|(index, action)| context_menu_row(action, index, cx)),
                ),
        )
        .when_some(submenu, |layer, (open, actions, x, y)| {
            layer.child(context_submenu_overlay(open, actions, x, y, cx))
        })
}

fn context_menu_actions(
    target: &ContextMenuTarget,
    clipboard_available: bool,
) -> Vec<ContextMenuItem> {
    match target {
        ContextMenuTarget::Blank {
            trash_view: true,
            trash_has_items,
        } => vec![
            ContextMenuItem {
                action: ContextMenuAction::EmptyTrash,
                label: "Empty Trash",
                enabled: *trash_has_items,
                submenu: None,
            },
            context_menu_submenu_item(
                ContextMenuAction::SortBySubmenu,
                "Sort By",
                ContextMenuSubmenu::SortBy,
            ),
            context_menu_submenu_item(
                ContextMenuAction::ViewModeSubmenu,
                "View Mode",
                ContextMenuSubmenu::ViewMode,
            ),
            context_menu_item(ContextMenuAction::SelectAll, "Select All"),
            context_menu_item(ContextMenuAction::Refresh, "Refresh"),
            context_menu_item(ContextMenuAction::Properties, "Properties"),
        ],
        ContextMenuTarget::Blank {
            trash_view: false, ..
        } => vec![
            context_menu_item(ContextMenuAction::CreateFolder, "New Folder"),
            ContextMenuItem {
                action: ContextMenuAction::Paste,
                label: "Paste",
                enabled: clipboard_available,
                submenu: None,
            },
            context_menu_submenu_item(
                ContextMenuAction::SortBySubmenu,
                "Sort By",
                ContextMenuSubmenu::SortBy,
            ),
            context_menu_submenu_item(
                ContextMenuAction::ViewModeSubmenu,
                "View Mode",
                ContextMenuSubmenu::ViewMode,
            ),
            context_menu_item(ContextMenuAction::SelectAll, "Select All"),
            context_menu_item(ContextMenuAction::Refresh, "Refresh"),
            context_menu_item(ContextMenuAction::Properties, "Properties"),
        ],
        ContextMenuTarget::PlacesBlank { hidden_sections } => {
            let mut actions = vec![context_menu_item(ContextMenuAction::AddPlace, "Add Entry")];
            actions.push(ContextMenuItem {
                action: ContextMenuAction::ShowHiddenPlaces,
                label: "Show Hidden Places",
                enabled: *hidden_sections,
                submenu: None,
            });
            actions
        }
        ContextMenuTarget::PlaceSection { .. } => {
            vec![context_menu_item(
                ContextMenuAction::HidePlaceSection,
                "Hide Section",
            )]
        }
        ContextMenuTarget::Place {
            trash_place: true,
            trash_has_items,
            ..
        } => vec![
            context_menu_item(ContextMenuAction::Open, "Open"),
            context_menu_item(ContextMenuAction::OpenInNewPane, "Open in New Pane"),
            ContextMenuItem {
                action: ContextMenuAction::EmptyTrash,
                label: "Empty Trash",
                enabled: *trash_has_items,
                submenu: None,
            },
            context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
            context_menu_item(ContextMenuAction::Properties, "Properties"),
        ],
        ContextMenuTarget::Place {
            editable,
            removable,
            ..
        } => vec![
            context_menu_item(ContextMenuAction::Open, "Open"),
            context_menu_item(ContextMenuAction::OpenInNewPane, "Open in New Pane"),
            context_menu_item_enabled(ContextMenuAction::EditPlace, "Edit Entry", *editable),
            context_menu_item_enabled(ContextMenuAction::RemovePlace, "Remove Entry", *removable),
            context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
            context_menu_item(ContextMenuAction::Properties, "Properties"),
        ],
        ContextMenuTarget::Item {
            trash_view: true,
            trash_can_restore,
            ..
        } => vec![
            ContextMenuItem {
                action: ContextMenuAction::RestoreFromTrash,
                label: "Restore to Former Location",
                enabled: *trash_can_restore,
                submenu: None,
            },
            context_menu_item(ContextMenuAction::Copy, "Copy"),
            context_menu_item(ContextMenuAction::DeletePermanently, "Delete Permanently"),
            context_menu_item(ContextMenuAction::Properties, "Properties"),
        ],
        ContextMenuTarget::Item {
            is_dir,
            selection_count,
            ..
        } if *selection_count > 1 => vec![
            context_menu_item(ContextMenuAction::Copy, "Copy"),
            context_menu_item(ContextMenuAction::Cut, "Cut"),
            context_menu_item(ContextMenuAction::Trash, "Move to Trash"),
            context_menu_item(ContextMenuAction::Properties, "Properties"),
        ],
        ContextMenuTarget::Item { is_dir, .. } => {
            let open_label = if *is_dir { "Open" } else { "Open With" };
            let mut actions = vec![context_menu_item(ContextMenuAction::Open, open_label)];
            if *is_dir {
                actions.push(context_menu_item(
                    ContextMenuAction::OpenInNewPane,
                    "Open in New Pane",
                ));
            }
            actions.extend([
                context_menu_item(ContextMenuAction::Cut, "Cut"),
                context_menu_item(ContextMenuAction::Copy, "Copy"),
                context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
            ]);
            if *is_dir {
                actions.push(ContextMenuItem {
                    action: ContextMenuAction::Paste,
                    label: "Paste",
                    enabled: clipboard_available,
                    submenu: None,
                });
            }
            actions.extend([
                context_menu_item(ContextMenuAction::Rename, "Rename"),
                context_menu_item(ContextMenuAction::Trash, "Move to Trash"),
                context_menu_item(ContextMenuAction::Properties, "Properties"),
            ]);
            actions
        }
    }
}

fn context_menu_item(action: ContextMenuAction, label: &'static str) -> ContextMenuItem {
    ContextMenuItem {
        action,
        label,
        enabled: true,
        submenu: None,
    }
}

fn context_menu_item_enabled(
    action: ContextMenuAction,
    label: &'static str,
    enabled: bool,
) -> ContextMenuItem {
    ContextMenuItem {
        action,
        label,
        enabled,
        submenu: None,
    }
}

fn context_menu_submenu_item(
    action: ContextMenuAction,
    label: &'static str,
    submenu: ContextMenuSubmenu,
) -> ContextMenuItem {
    ContextMenuItem {
        action,
        label,
        enabled: true,
        submenu: Some(submenu),
    }
}

fn disabled_context_menu_item(action: ContextMenuAction, label: &'static str) -> ContextMenuItem {
    ContextMenuItem {
        action,
        label,
        enabled: false,
        submenu: None,
    }
}

fn sort_role_label(role: SortRole) -> &'static str {
    match role {
        SortRole::Name => "Name",
        SortRole::Modified => "Modified",
        SortRole::Size => "Size",
        SortRole::TrashDeletionTime => "Deletion Time",
    }
}

fn sort_order_label(order: SortOrder) -> &'static str {
    match order {
        SortOrder::Ascending => "Ascending",
        SortOrder::Descending => "Descending",
    }
}

fn context_submenu_actions(submenu: ContextMenuSubmenu) -> Vec<ContextMenuItem> {
    match submenu {
        ContextMenuSubmenu::SortBy => vec![
            context_menu_item(ContextMenuAction::SortByName, "Name"),
            context_menu_item(ContextMenuAction::SortByModified, "Modified"),
            context_menu_item(ContextMenuAction::SortBySize, "Size"),
            context_menu_item(ContextMenuAction::SortAscending, "Ascending"),
            context_menu_item(ContextMenuAction::SortDescending, "Descending"),
            context_menu_item(ContextMenuAction::SortFoldersFirst, "Folders First"),
            context_menu_item(ContextMenuAction::SortHiddenLast, "Hidden Files Last"),
        ],
        ContextMenuSubmenu::ViewMode => vec![
            context_menu_item(ContextMenuAction::ViewCompact, "Compact"),
            disabled_context_menu_item(ContextMenuAction::ViewIcons, "Icons"),
            disabled_context_menu_item(ContextMenuAction::ViewDetails, "Details"),
        ],
    }
}

fn context_submenu_overlay(
    open: ContextMenuOpenSubmenu,
    actions: Vec<ContextMenuItem>,
    x: f32,
    y: f32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!(
            "context-submenu-{:?}-{}",
            open.submenu, open.parent_index
        ))
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(CONTEXT_MENU_WIDTH))
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xffffff))
        .shadow_md()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .children(
            actions
                .into_iter()
                .enumerate()
                .map(|(index, item)| context_menu_row(item, index, cx)),
        )
}

fn context_menu_row(
    item: ContextMenuItem,
    index: usize,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let action = item.action;
    let submenu = item.submenu;
    let mut row = div()
        .id(format!("context-menu-action-{action:?}"))
        .flex()
        .items_center()
        .justify_between()
        .h(px(CONTEXT_MENU_ROW_HEIGHT))
        .px_3()
        .gap_2()
        .text_sm()
        .text_color(if item.enabled {
            rgb(0x24292f)
        } else {
            rgb(0x9aa4b2)
        })
        .when(item.enabled, |row| {
            row.hover(|row| row.bg(rgb(0xeaf1ff)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    if let Some(submenu) = submenu {
                        this.open_context_submenu(submenu, index);
                    } else {
                        this.run_context_menu_action(action, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }))
        })
        .child(div().flex_1().truncate().child(item.label))
        .when(item.submenu.is_some(), |row| {
            row.child(div().text_color(rgb(0x6b7280)).child(">"))
        });

    if let Some(submenu) = item.submenu {
        row = row.on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
            if *hovered {
                this.open_context_submenu(submenu, index);
                cx.notify();
            }
        }));
    } else if item.enabled {
        row = row.on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
            if *hovered {
                this.clear_context_submenu();
                cx.notify();
            }
        }));
    }
    row
}

fn properties_dialog_overlay(
    dialog: PropertiesDialogState,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let title = dialog.title;
    div()
        .id("properties-dialog-layer")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x00000066))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_properties_dialog();
                cx.notify();
            }),
        )
        .child(
            div()
                .id("properties-dialog")
                .w(px(440.0))
                .rounded_md()
                .border_1()
                .border_color(rgb(0xc8ced6))
                .bg(rgb(0xffffff))
                .shadow_md()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(0xd5d9df))
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(0x1f2328))
                                .child(title),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0x59636e))
                                .hover(|button| button.bg(rgb(0xeaf1ff)))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        |this, _event: &gpui::MouseDownEvent, _window, cx| {
                                            this.dismiss_properties_dialog();
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    ),
                                )
                                .child("Close"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .px_4()
                        .py_3()
                        .children(dialog.rows.into_iter().map(property_dialog_row)),
                ),
        )
}

fn place_draft_overlay(draft: PlaceDraft, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let title = if draft.editing_path.is_some() {
        "Edit Place"
    } else {
        "Add Place"
    };
    div()
        .id("place-draft-layer")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x00000066))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_place_draft();
                cx.notify();
            }),
        )
        .child(
            div()
                .id("place-draft-dialog")
                .w(px(460.0))
                .rounded_md()
                .border_1()
                .border_color(rgb(0xc8ced6))
                .bg(rgb(0xffffff))
                .shadow_md()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(0xd5d9df))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(0x1f2328))
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .px_4()
                        .py_3()
                        .child(place_draft_field(
                            PlaceDraftField::Label,
                            "Label",
                            draft.label,
                            draft.focus == PlaceDraftField::Label,
                            cx,
                        ))
                        .child(place_draft_field(
                            PlaceDraftField::Path,
                            "Path",
                            draft.path,
                            draft.focus == PlaceDraftField::Path,
                            cx,
                        ))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .pt_1()
                                .child(
                                    div()
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .text_sm()
                                        .text_color(rgb(0x59636e))
                                        .hover(|button| button.bg(rgb(0xeaf1ff)))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this,
                                                 _event: &gpui::MouseDownEvent,
                                                 _window,
                                                 cx| {
                                                    this.dismiss_place_draft();
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                },
                                            ),
                                        )
                                        .child("Cancel"),
                                )
                                .child(
                                    div()
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .bg(rgb(0x2f6fed))
                                        .text_sm()
                                        .text_color(rgb(0xffffff))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this,
                                                 _event: &gpui::MouseDownEvent,
                                                 _window,
                                                 cx| {
                                                    this.commit_place_draft();
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                },
                                            ),
                                        )
                                        .child("Save"),
                                ),
                        ),
                ),
        )
}

fn place_draft_field(
    field: PlaceDraftField,
    label: &'static str,
    value: String,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("place-draft-field-{field:?}"))
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(rgb(0x6b7280)).child(label))
        .child(
            div()
                .min_h(px(30.0))
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(if focused {
                    rgb(0x2f6fed)
                } else {
                    rgb(0xc8ced6)
                })
                .bg(if focused {
                    rgb(0xf3f7ff)
                } else {
                    rgb(0xffffff)
                })
                .text_sm()
                .text_color(rgb(0x24292f))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.set_place_draft_focus(field);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(if focused { format!("{value}|") } else { value }),
        )
}

fn property_dialog_row(row: PropertyRow) -> Stateful<Div> {
    div()
        .id(format!("property-row-{}", row.label))
        .flex()
        .items_center()
        .gap_3()
        .py_1()
        .child(
            div()
                .w(px(92.0))
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child(row.label),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_sm()
                .text_color(rgb(0x24292f))
                .child(row.value),
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaneShortcut {
    SelectAll,
    ClearSelection,
    Refresh,
    GoParent,
    GoBack,
    GoForward,
    SplitPane,
    ClosePane,
    MoveSelection {
        direction: SelectionMove,
        extend: bool,
    },
    CreateFolder,
    RenameSelection,
    EditLocation,
    ShowFilter,
    Zoom(ZoomChange),
    CopySelection,
    CutSelection,
    PasteIntoPane,
    TrashSelection,
    Undo,
}

fn pane_shortcut(keystroke: &gpui::Keystroke) -> Option<PaneShortcut> {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => Some(PaneShortcut::ClearSelection),
            "/" => Some(PaneShortcut::ShowFilter),
            "f5" => Some(PaneShortcut::Refresh),
            "f6" => Some(PaneShortcut::EditLocation),
            "f3" => Some(PaneShortcut::SplitPane),
            "f2" => Some(PaneShortcut::RenameSelection),
            "up" | "left" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: false,
            }),
            "down" | "right" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: false,
            }),
            "backspace" => Some(PaneShortcut::GoParent),
            "delete" => Some(PaneShortcut::TrashSelection),
            _ => None,
        };
    }

    if keystroke.modifiers.shift && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "up" | "left" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: true,
            }),
            "down" | "right" => Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: true,
            }),
            _ => None,
        };
    }

    if keystroke.modifiers.secondary()
        && keystroke.modifiers.shift
        && keystroke.modifiers.number_of_modifiers() == 2
    {
        if let Some(shortcut) = zoom_shortcut(keystroke) {
            return Some(shortcut);
        }
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "n" => Some(PaneShortcut::CreateFolder),
            _ => None,
        };
    }

    if keystroke.modifiers.secondary() && keystroke.modifiers.number_of_modifiers() == 1 {
        if let Some(shortcut) = zoom_shortcut(keystroke) {
            return Some(shortcut);
        }
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "a" => Some(PaneShortcut::SelectAll),
            "c" => Some(PaneShortcut::CopySelection),
            "i" => Some(PaneShortcut::ShowFilter),
            "l" => Some(PaneShortcut::EditLocation),
            "v" => Some(PaneShortcut::PasteIntoPane),
            "w" => Some(PaneShortcut::ClosePane),
            "x" => Some(PaneShortcut::CutSelection),
            "z" => Some(PaneShortcut::Undo),
            _ => None,
        };
    }

    if keystroke.modifiers.alt && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "d" => Some(PaneShortcut::EditLocation),
            "left" => Some(PaneShortcut::GoBack),
            "right" => Some(PaneShortcut::GoForward),
            _ => None,
        };
    }

    None
}

fn zoom_shortcut(keystroke: &gpui::Keystroke) -> Option<PaneShortcut> {
    let key = keystroke.key.to_ascii_lowercase();
    let key_char = keystroke.key_char.as_deref();
    if matches!(key.as_str(), "+" | "=" | "plus") || key_char == Some("+") {
        return Some(PaneShortcut::Zoom(ZoomChange::In));
    }
    if matches!(key.as_str(), "-" | "minus") || key_char == Some("-") {
        return Some(PaneShortcut::Zoom(ZoomChange::Out));
    }
    if key == "0" || key_char == Some("0") {
        return Some(PaneShortcut::Zoom(ZoomChange::Reset));
    }
    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RenameInputAction {
    Cancel,
    Commit,
    Backspace,
    Insert(String),
    Ignore,
}

fn rename_input_action(keystroke: &gpui::Keystroke) -> RenameInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => RenameInputAction::Cancel,
            "enter" => RenameInputAction::Commit,
            "backspace" => RenameInputAction::Backspace,
            _ => rename_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return rename_text_input_action(keystroke);
    }

    RenameInputAction::Ignore
}

fn rename_text_input_action(keystroke: &gpui::Keystroke) -> RenameInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| RenameInputAction::Insert(text.clone()))
        .unwrap_or(RenameInputAction::Ignore)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LocationInputAction {
    Cancel,
    Commit,
    Complete,
    Backspace,
    Insert(String),
    Ignore,
}

fn location_input_action(keystroke: &gpui::Keystroke) -> LocationInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => LocationInputAction::Cancel,
            "enter" => LocationInputAction::Commit,
            "tab" => LocationInputAction::Complete,
            "backspace" => LocationInputAction::Backspace,
            _ => location_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return location_text_input_action(keystroke);
    }

    LocationInputAction::Ignore
}

fn location_text_input_action(keystroke: &gpui::Keystroke) -> LocationInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| LocationInputAction::Insert(text.clone()))
        .unwrap_or(LocationInputAction::Ignore)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PlaceInputAction {
    Cancel,
    Commit,
    NextField,
    Backspace,
    Insert(String),
    Ignore,
}

fn place_input_action(keystroke: &gpui::Keystroke) -> PlaceInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => PlaceInputAction::Cancel,
            "enter" => PlaceInputAction::Commit,
            "tab" => PlaceInputAction::NextField,
            "backspace" => PlaceInputAction::Backspace,
            _ => place_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return place_text_input_action(keystroke);
    }

    PlaceInputAction::Ignore
}

fn place_text_input_action(keystroke: &gpui::Keystroke) -> PlaceInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| PlaceInputAction::Insert(text.clone()))
        .unwrap_or(PlaceInputAction::Ignore)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FilterInputAction {
    Cancel,
    FocusView,
    Backspace,
    Insert(String),
    PassToView,
    Ignore,
}

fn filter_input_action(keystroke: &gpui::Keystroke) -> FilterInputAction {
    if has_no_modifiers(keystroke) {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "escape" => FilterInputAction::Cancel,
            "enter" => FilterInputAction::FocusView,
            "up" | "down" | "pageup" | "pagedown" => FilterInputAction::PassToView,
            "backspace" => FilterInputAction::Backspace,
            _ => filter_text_input_action(keystroke),
        };
    }

    if keystroke.modifiers.shift
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
    {
        return filter_text_input_action(keystroke);
    }

    FilterInputAction::Ignore
}

fn filter_text_input_action(keystroke: &gpui::Keystroke) -> FilterInputAction {
    keystroke
        .key_char
        .as_ref()
        .filter(|text| text.chars().all(|ch| !ch.is_control()))
        .map(|text| FilterInputAction::Insert(text.clone()))
        .unwrap_or(FilterInputAction::Ignore)
}

fn has_no_modifiers(keystroke: &gpui::Keystroke) -> bool {
    !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.shift
        && !keystroke.modifiers.platform
        && !keystroke.modifiers.function
}

#[derive(Clone, Debug)]
struct TrashSelectionResult {
    pane_id: PaneId,
    success_count: usize,
    failure_count: usize,
    affected_dirs: Vec<PathBuf>,
    undo_items: Vec<TrashUndoItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrashViewOperation {
    Restore,
    DeletePermanently,
    Empty,
}

impl TrashViewOperation {
    fn progress_label(self, count: usize) -> String {
        match self {
            Self::Restore => format!("Restoring {count} item(s)"),
            Self::DeletePermanently => format!("Deleting {count} item(s) permanently"),
            Self::Empty => "Emptying Trash".to_string(),
        }
    }

    fn completed_label(self) -> &'static str {
        match self {
            Self::Restore => "Restored from trash",
            Self::DeletePermanently => "Deleted permanently",
            Self::Empty => "Emptied Trash",
        }
    }
}

#[derive(Clone, Debug)]
struct TrashViewOperationResult {
    pane_id: PaneId,
    operation: TrashViewOperation,
    success_count: usize,
    failure_count: usize,
    affected_dirs: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct PasteTaskResult {
    pane_id: PaneId,
    mode: ClipboardMode,
    label: &'static str,
    success_count: usize,
    failure_count: usize,
    affected_dirs: Vec<PathBuf>,
    undo_items: Vec<TransferUndoItem>,
    created_items: Vec<CreateUndoItem>,
}

#[derive(Clone, Debug)]
struct RenameItemResult {
    pane_id: PaneId,
    original_path: PathBuf,
    affected_dirs: Vec<PathBuf>,
    result: Result<PathBuf, String>,
}

#[derive(Clone, Debug)]
struct CreateItemResult {
    pane_id: PaneId,
    kind: CreatedItemKind,
    affected_dirs: Vec<PathBuf>,
    result: Result<PathBuf, String>,
}

fn rename_item_result(
    pane_id: PaneId,
    original_path: PathBuf,
    new_name: String,
) -> RenameItemResult {
    let mut affected_dirs = parent_dirs([original_path.clone()]);
    let result = file_ops::rename_path(&original_path, &new_name);
    if let Ok(renamed_path) = &result
        && let Some(parent) = renamed_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    {
        push_unique_path(&mut affected_dirs, parent.to_path_buf());
    }

    RenameItemResult {
        pane_id,
        original_path,
        affected_dirs,
        result,
    }
}

fn create_item_result(
    pane_id: PaneId,
    parent_dir: PathBuf,
    kind: CreatedItemKind,
) -> CreateItemResult {
    let result = match kind {
        CreatedItemKind::File => {
            file_ops::create_file(&parent_dir, default_created_item_name(kind))
        }
        CreatedItemKind::Folder => {
            file_ops::create_folder(&parent_dir, default_created_item_name(kind))
        }
    };
    CreateItemResult {
        pane_id,
        kind,
        affected_dirs: vec![parent_dir],
        result,
    }
}

fn default_created_item_name(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "New File.txt",
        CreatedItemKind::Folder => "New Folder",
    }
}

fn created_item_label(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "File",
        CreatedItemKind::Folder => "Folder",
    }
}

fn paste_clipboard_result(
    pane_id: PaneId,
    target_dir: PathBuf,
    clipboard: ClipboardState,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<Mutex<file_ops::TransferProgress>>>,
) -> PasteTaskResult {
    let mode = clipboard.mode;
    let label = clipboard.action_label();
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut undo_items = Vec::new();
    let mut created_items = Vec::new();

    if let Some(text) = clipboard.text.as_deref() {
        let result =
            file_ops::write_unique_file(&target_dir, "Pasted Text", "txt", text.as_bytes());
        match result {
            Ok(path) => {
                success_count = 1;
                push_unique_path(&mut affected_dirs, target_dir.clone());
                created_items.push(CreateUndoItem {
                    path,
                    kind: CreatedItemKind::File,
                });
            }
            Err(_) => {
                failure_count = 1;
            }
        }
        return PasteTaskResult {
            pane_id,
            mode,
            label,
            success_count,
            failure_count,
            affected_dirs,
            undo_items,
            created_items,
        };
    }

    let operation = mode.operation();

    for source in clipboard.paths {
        if cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            failure_count += 1;
            continue;
        }
        let progress = progress.clone();
        match file_ops::perform_transfer_with_progress_outcome(
            operation,
            &source,
            &target_dir,
            "keep-both",
            cancel.clone(),
            move |transfer_progress| {
                if let Some(progress) = &progress
                    && let Ok(mut progress) = progress.lock()
                {
                    *progress = transfer_progress;
                }
            },
        ) {
            Ok(outcome) => {
                success_count += 1;
                push_unique_path(&mut affected_dirs, target_dir.clone());
                if mode == ClipboardMode::Cut
                    && let Some(parent) = source
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                {
                    push_unique_path(&mut affected_dirs, parent.to_path_buf());
                }
                undo_items.push(TransferUndoItem {
                    operation: operation.to_string(),
                    original_source: source,
                    destination: outcome.destination,
                    overwritten_backup: outcome.overwritten_backup,
                });
            }
            Err(_) => {
                failure_count += 1;
            }
        }
    }

    PasteTaskResult {
        pane_id,
        mode,
        label,
        success_count,
        failure_count,
        affected_dirs,
        undo_items,
        created_items,
    }
}

fn trash_selection_result(pane_id: PaneId, selected_paths: Vec<PathBuf>) -> TrashSelectionResult {
    let summary = file_ops::trash_paths(&selected_paths);
    let success_count = summary.successes.len();
    let failure_count = summary.failures.len();
    let undo_items = summary
        .successes
        .iter()
        .map(|record| TrashUndoItem {
            original_path: record.original_path.clone(),
            trash_path: record.trash_path.clone(),
        })
        .collect::<Vec<_>>();
    let mut affected_dirs = parent_dirs(
        summary
            .successes
            .iter()
            .map(|record| record.original_path.clone()),
    );
    if success_count > 0 {
        push_unique_path(&mut affected_dirs, file_ops::trash_files_dir());
    }

    TrashSelectionResult {
        pane_id,
        success_count,
        failure_count,
        affected_dirs,
        undo_items,
    }
}

fn trash_view_operation_result(
    pane_id: PaneId,
    operation: TrashViewOperation,
    paths: Vec<PathBuf>,
) -> TrashViewOperationResult {
    let summary = match operation {
        TrashViewOperation::Restore => file_ops::restore_trash_paths(&paths),
        TrashViewOperation::DeletePermanently => file_ops::permanently_delete_trash_paths(&paths),
        TrashViewOperation::Empty => file_ops::empty_trash(),
    };
    let success_count = summary.successes.len();
    let failure_count = summary.failures.len();
    let mut affected_dirs = Vec::new();
    if success_count > 0 {
        push_unique_path(&mut affected_dirs, file_ops::trash_files_dir());
    }
    if operation == TrashViewOperation::Restore {
        for record in &summary.successes {
            if let Some(parent) = record
                .original_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                push_unique_path(&mut affected_dirs, parent.to_path_buf());
            }
        }
    }

    TrashViewOperationResult {
        pane_id,
        operation,
        success_count,
        failure_count,
        affected_dirs,
    }
}

#[derive(Clone, Debug)]
struct UndoTaskResult {
    record: UndoRecord,
    result: Result<String, String>,
}

fn undo_record_result(record: UndoRecord) -> UndoTaskResult {
    let result = match &record.payload {
        UndoPayload::Create { items } => {
            let mut removed_count = 0;
            for item in items {
                let result = match item.kind {
                    CreatedItemKind::File => file_ops::undo_create_file(&item.path),
                    CreatedItemKind::Folder => file_ops::undo_create_folder(&item.path),
                };
                if let Err(err) = result {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "removed {removed_count} item(s), then failed: {err}"
                        )),
                    };
                }
                removed_count += 1;
            }
            Ok(format!("removed {} item(s)", items.len()))
        }
        UndoPayload::Trash { items } => {
            let restore_pairs = items
                .iter()
                .map(|item| (item.original_path.clone(), item.trash_path.clone()))
                .collect::<Vec<_>>();
            file_ops::undo_trash(&restore_pairs)
        }
        UndoPayload::Rename { items } => {
            let mut restored_count = 0;
            for item in items {
                if let Err(err) = file_ops::undo_rename(&item.original_path, &item.renamed_path) {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "restored {restored_count} item(s), then failed: {err}"
                        )),
                    };
                }
                restored_count += 1;
            }
            Ok(format!("restored {} item(s)", items.len()))
        }
        UndoPayload::Transfer { items } => {
            let mut restored_count = 0;
            for item in items {
                if let Err(err) = file_ops::undo_transfer_with_backup(
                    &item.operation,
                    &item.original_source,
                    &item.destination,
                    item.overwritten_backup.as_deref(),
                ) {
                    return UndoTaskResult {
                        record,
                        result: Err(format!(
                            "restored {restored_count} item(s), then failed: {err}"
                        )),
                    };
                }
                restored_count += 1;
            }
            Ok(format!("restored {} item(s)", items.len()))
        }
        UndoPayload::None => Err(format!("no undo action for {}", record.label)),
    };
    UndoTaskResult { record, result }
}

fn parent_dirs(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for path in paths {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            push_unique_path(&mut dirs, parent.to_path_buf());
        }
    }
    dirs
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn action_status(label: &str, success_count: usize, failure_count: usize) -> String {
    match (success_count, failure_count) {
        (0, 0) => format!("{label}: no changes"),
        (_, 0) => format!("{label}: {success_count} item(s)"),
        (0, _) => format!("{label} failed for {failure_count} item(s)"),
        (_, _) => format!("{label}: {success_count} item(s), {failure_count} failed"),
    }
}

fn status_summary_for_model(
    entries: &[fika_core::ModelEntry],
    selection: &fika_core::SelectionState,
) -> String {
    let has_selection = !selection.is_empty();
    let mut folders = 0usize;
    let mut files = 0usize;
    let mut total_size = 0u64;

    for entry in entries {
        if has_selection && !selection.is_selected(entry.id) {
            continue;
        }
        if entry.is_dir {
            folders += 1;
        } else {
            files += 1;
            total_size = total_size.saturating_add(entry.size_bytes);
        }
    }

    format_status_counts(folders, files, total_size, has_selection)
}

fn status_summary_for_model_indexes(
    entries: &[fika_core::ModelEntry],
    indexes: impl IntoIterator<Item = usize>,
    selection: &fika_core::SelectionState,
) -> String {
    let has_selection = !selection.is_empty();
    let mut folders = 0usize;
    let mut files = 0usize;
    let mut total_size = 0u64;

    for index in indexes {
        let Some(entry) = entries.get(index) else {
            continue;
        };
        if has_selection && !selection.is_selected(entry.id) {
            continue;
        }
        if entry.is_dir {
            folders += 1;
        } else {
            files += 1;
            total_size = total_size.saturating_add(entry.size_bytes);
        }
    }

    format_status_counts(folders, files, total_size, has_selection)
}

fn format_status_counts(
    folders: usize,
    files: usize,
    total_size: u64,
    has_selection: bool,
) -> String {
    let folder_label = count_label(
        folders,
        if has_selection {
            "folder selected"
        } else {
            "folder"
        },
    );
    let file_label = count_label(
        files,
        if has_selection {
            "file selected"
        } else {
            "file"
        },
    );

    match (folders, files) {
        (0, 0) => "0 folders, 0 files".to_string(),
        (_, 0) => folder_label,
        (0, _) => format!("{file_label} ({})", fika_core::format_size(total_size)),
        _ => format!(
            "{folder_label}, {file_label} ({})",
            fika_core::format_size(total_size)
        ),
    }
}

fn count_label(count: usize, singular: &'static str) -> String {
    let suffix = if count == 1 {
        singular
    } else {
        match singular {
            "folder" => "folders",
            "file" => "files",
            "folder selected" => "folders selected",
            "file selected" => "files selected",
            _ => singular,
        }
    };
    format!("{count} {suffix}")
}

fn filesystem_space_info(path: PathBuf) -> Option<SpaceInfoSnapshot> {
    let output = Command::new("df")
        .arg("-B1")
        .arg("--output=size,avail")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_df_space_output(std::str::from_utf8(&output.stdout).ok()?)
}

fn parse_df_space_output(output: &str) -> Option<SpaceInfoSnapshot> {
    let values = output.lines().skip(1).find_map(|line| {
        let mut parts = line.split_whitespace();
        let total = parts.next()?.parse::<u64>().ok()?;
        let available = parts.next()?.parse::<u64>().ok()?;
        Some((total, available))
    })?;
    space_info_snapshot(values.0, values.1)
}

fn space_info_snapshot(total: u64, available: u64) -> Option<SpaceInfoSnapshot> {
    if total == 0 {
        return None;
    }
    let available = available.min(total);
    let used = total.saturating_sub(available);
    let used_percent = ((used.saturating_mul(100) + (total / 2)) / total).min(100) as u8;
    Some(SpaceInfoSnapshot {
        free_label: format!("{} free", fika_core::format_size(available)),
        detail_label: format!(
            "{} free out of {} ({}% used)",
            fika_core::format_size(available),
            fika_core::format_size(total),
            used_percent
        ),
        used_percent,
    })
}

fn properties_for_path(path: &Path) -> PropertiesDialogState {
    let title_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string());
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) => {
            return PropertiesDialogState {
                title: format!("Properties - {title_name}"),
                rows: vec![
                    property_row("Name", title_name),
                    property_row("Path", path.display().to_string()),
                    property_row("Status", format!("Cannot read metadata: {err}")),
                ],
            };
        }
    };

    let location = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    let size = if metadata.is_dir() {
        "-".to_string()
    } else {
        fika_core::format_size(metadata.len())
    };

    PropertiesDialogState {
        title: format!("Properties - {title_name}"),
        rows: vec![
            property_row("Name", title_name),
            property_row("Type", property_type_label(&metadata).to_string()),
            property_row("Location", location),
            property_row("Size", size),
            property_row("Modified", format_metadata_modified(&metadata)),
            property_row("Path", path.display().to_string()),
        ],
    }
}

fn properties_for_selection(paths: &[PathBuf]) -> PropertiesDialogState {
    let mut files = 0usize;
    let mut folders = 0usize;
    let mut links = 0usize;
    let mut unreadable = 0usize;
    let mut total_size = 0u64;
    let mut common_parent: Option<PathBuf> = None;

    for path in paths {
        common_parent = common_parent_path(common_parent, path.parent().map(Path::to_path_buf));
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    links += 1;
                } else if metadata.is_dir() {
                    folders += 1;
                } else {
                    files += 1;
                    total_size = total_size.saturating_add(metadata.len());
                }
            }
            Err(_) => unreadable += 1,
        }
    }

    let mut type_parts = Vec::new();
    push_count_label(&mut type_parts, folders, "folder");
    push_count_label(&mut type_parts, files, "file");
    push_count_label(&mut type_parts, links, "link");
    push_count_label(&mut type_parts, unreadable, "unreadable item");
    if type_parts.is_empty() {
        type_parts.push("no readable items".to_string());
    }

    let mut rows = vec![
        property_row("Items", paths.len().to_string()),
        property_row("Type", type_parts.join(", ")),
        property_row("Size", fika_core::format_size(total_size)),
    ];
    if let Some(parent) = common_parent {
        rows.push(property_row("Location", parent.display().to_string()));
    }

    PropertiesDialogState {
        title: format!("Properties - {} items", paths.len()),
        rows,
    }
}

fn property_row(label: &'static str, value: String) -> PropertyRow {
    PropertyRow { label, value }
}

fn property_type_label(metadata: &fs::Metadata) -> &'static str {
    if metadata.file_type().is_symlink() {
        "Symbolic Link"
    } else if metadata.is_dir() {
        "Folder"
    } else if metadata.is_file() {
        "File"
    } else {
        "Special File"
    }
}

fn format_metadata_modified(metadata: &fs::Metadata) -> String {
    fika_core::format_modified_secs(
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs()),
    )
}

fn common_parent_path(current: Option<PathBuf>, candidate: Option<PathBuf>) -> Option<PathBuf> {
    match (current, candidate) {
        (None, next) => next,
        (Some(current), Some(candidate)) if current == candidate => Some(current),
        (Some(_), Some(_)) | (Some(_), None) => None,
    }
}

fn push_count_label(parts: &mut Vec<String>, count: usize, singular: &'static str) {
    if count == 0 {
        return;
    }
    let suffix = if count == 1 {
        singular
    } else {
        plural_label(singular)
    };
    parts.push(format!("{count} {suffix}"));
}

fn plural_label(singular: &'static str) -> &'static str {
    match singular {
        "folder" => "folders",
        "file" => "files",
        "link" => "links",
        "unreadable item" => "unreadable items",
        _ => singular,
    }
}

fn selected_choice_rows(specs: &[String]) -> Vec<String> {
    specs
        .iter()
        .filter_map(|spec| {
            let mut parts = spec.split('\t');
            let id = parts.next()?;
            let _label = parts.next()?;
            let default = parts.next().unwrap_or_default();
            let options = parts.next().unwrap_or_default();
            let selected = if default.is_empty() {
                options
                    .split(';')
                    .next()
                    .and_then(|option| option.split_once('=').map(|(value, _)| value))
                    .unwrap_or_default()
            } else {
                default
            };
            (!id.is_empty() && !selected.is_empty())
                .then(|| format!("FIKA_CHOOSER_CHOICE\t{id}\t{selected}"))
        })
        .collect()
}

fn build_places(user_places_path: &Path) -> Vec<PlaceEntry> {
    let home = home_dir();
    let mut places = Vec::new();
    push_place(&mut places, "", "H", "Home", home.clone());
    push_existing_place(&mut places, "", "Desk", "Desktop", home.join("Desktop"));
    push_existing_place(&mut places, "", "Doc", "Documents", home.join("Documents"));
    push_existing_place(&mut places, "", "Down", "Downloads", home.join("Downloads"));
    push_existing_place(&mut places, "", "Mus", "Music", home.join("Music"));
    push_existing_place(&mut places, "", "Pic", "Pictures", home.join("Pictures"));
    push_existing_place(&mut places, "", "Vid", "Videos", home.join("Videos"));
    push_place(&mut places, "", "Tr", "Trash", file_ops::trash_files_dir());
    let built_in_paths = places
        .iter()
        .map(|place| place.path.clone())
        .chain(std::iter::once(PathBuf::from("/")))
        .collect::<BTreeSet<_>>();
    for place in fika_core::load_user_places(user_places_path).unwrap_or_default() {
        if !built_in_paths.contains(&place.path) {
            push_user_place(&mut places, place.label, place.path);
        }
    }
    push_place(&mut places, "Devices", "/", "Root", PathBuf::from("/"));
    places
}

fn push_existing_place(
    places: &mut Vec<PlaceEntry>,
    group: &'static str,
    marker: &'static str,
    label: &'static str,
    path: PathBuf,
) {
    if path.is_dir() {
        push_place(places, group, marker, label, path);
    }
}

fn push_place(
    places: &mut Vec<PlaceEntry>,
    group: &'static str,
    marker: &'static str,
    label: &'static str,
    path: PathBuf,
) {
    if places.iter().any(|place| place.path == path) {
        return;
    }
    places.push(PlaceEntry {
        group,
        marker,
        label: label.to_string(),
        path,
        editable: false,
        removable: false,
    });
}

fn push_user_place(places: &mut Vec<PlaceEntry>, label: String, path: PathBuf) {
    if places.iter().any(|place| place.path == path) {
        return;
    }
    places.push(PlaceEntry {
        group: "",
        marker: "B",
        label,
        path,
        editable: true,
        removable: true,
    });
}

fn default_place_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

fn active_place_index(places: &[PlaceEntry], current_dir: &Path) -> Option<usize> {
    places
        .iter()
        .enumerate()
        .filter(|(_, place)| current_dir == place.path || current_dir.starts_with(&place.path))
        .max_by_key(|(_, place)| place.path.components().count())
        .map(|(index, _)| index)
}

fn breadcrumb_segments(path: &Path) -> Vec<BreadcrumbSegment> {
    let mut segments = Vec::new();
    let mut current = PathBuf::new();

    for component in path.components() {
        let label = match component {
            Component::Prefix(prefix) => {
                current.push(prefix.as_os_str());
                prefix.as_os_str().to_string_lossy().into_owned()
            }
            Component::RootDir => {
                current = PathBuf::from("/");
                "/".to_string()
            }
            Component::CurDir => {
                current.push(".");
                ".".to_string()
            }
            Component::ParentDir => {
                current.push("..");
                "..".to_string()
            }
            Component::Normal(name) => {
                current.push(name);
                name.to_string_lossy().into_owned()
            }
        };
        segments.push(BreadcrumbSegment {
            label,
            path: current.clone(),
        });
    }

    if segments.is_empty() {
        segments.push(BreadcrumbSegment {
            label: ".".to_string(),
            path: PathBuf::from("."),
        });
    }

    segments
}

fn resolve_location_input(current_dir: &Path, input: &str) -> Option<PathBuf> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let expanded = expand_user_path(input);
    if expanded.is_absolute() {
        Some(expanded)
    } else {
        Some(current_dir.join(expanded))
    }
}

fn complete_location_input(current_dir: &Path, input: &str) -> Option<String> {
    let (parent_text, prefix) = split_location_input(input);
    let parent = if parent_text.is_empty() {
        current_dir.to_path_buf()
    } else {
        resolve_location_input(current_dir, parent_text)?
    };
    let mut matches = fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(prefix)
                .then(|| (name, entry.file_type().ok().is_some_and(|ty| ty.is_dir())))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.0.cmp(&right.0));

    let (name, is_dir) = matches.into_iter().next()?;
    let mut completed = join_location_text(parent_text, &name);
    if is_dir && !completed.ends_with('/') {
        completed.push('/');
    }
    Some(completed)
}

fn split_location_input(input: &str) -> (&str, &str) {
    let input = input.trim();
    match input.rfind('/') {
        Some(0) => ("/", &input[1..]),
        Some(index) => (&input[..index], &input[index + 1..]),
        None => ("", input),
    }
}

fn join_location_text(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn normalize_start_dir(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        path
    } else {
        path.parent()
            .map(|parent| {
                if parent.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    parent.to_path_buf()
                }
            })
            .unwrap_or_else(home_dir)
    }
}

fn expand_user_path(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn print_help() {
    println!(
        "Usage: fika [--chooser] [START_DIR]\n\n\
         Options:\n\
           --chooser                 Start the GPUI file chooser shell.\n\
           --chooser-directory       Select folders instead of files.\n\
           --chooser-multiple        Select more than one path before confirmation.\n\
           --chooser-title TITLE     Use TITLE as the chooser window title.\n\
           --chooser-accept-label L  Use L in the chooser chrome.\n\
           --chooser-filter-index N  Return N as selected filter metadata.\n\
           --chooser-return-filter   Print selected filter metadata before paths.\n\
           --chooser-choices LIST    Preserve portal choice metadata.\n\
           --chooser-return-choices  Print selected choice metadata before paths.\n\
           -h, --help                Show this help."
    );
}

fn main() {
    let args = Args::parse(env::args().skip(1));
    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| FikaApp::new(args.clone(), cx)),
        )
        .expect("failed to open Fika GPUI window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooser_choice_defaults_are_returned_without_ui_state() {
        let rows = selected_choice_rows(&[
            "encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1".to_string()
        ]);
        assert_eq!(rows, vec!["FIKA_CHOOSER_CHOICE\tencoding\tutf8"]);
    }

    #[test]
    fn chooser_choice_falls_back_to_first_option() {
        let rows = selected_choice_rows(&["quality\tQuality\t\tlow=Low;high=High".to_string()]);
        assert_eq!(rows, vec!["FIKA_CHOOSER_CHOICE\tquality\tlow"]);
    }

    #[test]
    fn parses_chooser_mode_without_versioned_dependencies() {
        let args = Args::parse(
            ["--chooser", "--chooser-directory", "/tmp"]
                .into_iter()
                .map(str::to_string),
        );
        assert_eq!(args.mode, Mode::Chooser);
        assert!(args.chooser_directories);
    }

    #[test]
    fn active_place_prefers_longest_path_prefix() {
        let places = vec![
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
            },
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: PathBuf::from("/home/yk"),
                editable: false,
                removable: false,
            },
            PlaceEntry {
                group: "",
                marker: "Down",
                label: "Downloads".to_string(),
                path: PathBuf::from("/home/yk/Downloads"),
                editable: false,
                removable: false,
            },
        ];

        assert_eq!(
            active_place_index(&places, Path::new("/home/yk/Downloads/archive")),
            Some(2)
        );
    }

    #[test]
    fn breadcrumb_segments_build_incremental_paths() {
        let segments = breadcrumb_segments(Path::new("/home/yk/Documents"));
        let labels = segments
            .iter()
            .map(|segment| segment.label.as_str())
            .collect::<Vec<_>>();
        let paths = segments
            .iter()
            .map(|segment| segment.path.clone())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["/", "home", "yk", "Documents"]);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/"),
                PathBuf::from("/home"),
                PathBuf::from("/home/yk"),
                PathBuf::from("/home/yk/Documents"),
            ]
        );
    }

    #[test]
    fn location_input_resolves_absolute_relative_and_home_paths() {
        let current = Path::new("/tmp/fika-current");

        assert_eq!(
            resolve_location_input(current, "/etc"),
            Some(PathBuf::from("/etc"))
        );
        assert_eq!(
            resolve_location_input(current, "notes"),
            Some(PathBuf::from("/tmp/fika-current/notes"))
        );
        assert_eq!(resolve_location_input(current, "  "), None);
        assert_eq!(resolve_location_input(current, "~"), Some(home_dir()));
    }

    #[test]
    fn location_completion_uses_filesystem_and_sorts_matches() {
        let temp = test_dir("location-completion");
        std::fs::create_dir_all(temp.join("alpha")).unwrap();
        std::fs::write(temp.join("alpine.txt"), "file").unwrap();
        std::fs::create_dir_all(temp.join("nested")).unwrap();
        std::fs::create_dir_all(temp.join("nested/zed")).unwrap();
        std::fs::create_dir_all(temp.join("nested/zen")).unwrap();

        assert_eq!(
            complete_location_input(&temp, "al"),
            Some("alpha/".to_string())
        );
        assert_eq!(
            complete_location_input(&temp, "nested/ze"),
            Some("nested/zed/".to_string())
        );
        assert_eq!(complete_location_input(&temp, "missing"), None);

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn context_menu_actions_track_blank_paste_availability() {
        let blank = context_blank_target();
        let without_clipboard = context_menu_actions(&blank, false);
        let with_clipboard = context_menu_actions(&blank, true);

        assert_eq!(
            without_clipboard
                .iter()
                .find(|item| item.action == ContextMenuAction::Paste)
                .map(|item| item.enabled),
            Some(false)
        );
        assert_eq!(
            with_clipboard
                .iter()
                .find(|item| item.action == ContextMenuAction::Paste)
                .map(|item| item.enabled),
            Some(true)
        );
        assert!(
            with_clipboard
                .iter()
                .any(|item| item.action == ContextMenuAction::Properties)
        );
    }

    #[test]
    fn context_menu_actions_offer_blank_sort_and_view_submenus() {
        let blank = context_blank_target();
        let actions = context_menu_actions(&blank, false);

        assert_eq!(
            actions
                .iter()
                .find(|item| item.action == ContextMenuAction::SortBySubmenu)
                .and_then(|item| item.submenu),
            Some(ContextMenuSubmenu::SortBy)
        );
        assert_eq!(
            actions
                .iter()
                .find(|item| item.action == ContextMenuAction::ViewModeSubmenu)
                .and_then(|item| item.submenu),
            Some(ContextMenuSubmenu::ViewMode)
        );
    }

    #[test]
    fn context_submenu_actions_enable_sort_but_keep_unimplemented_view_modes_disabled() {
        let sort_actions = context_submenu_actions(ContextMenuSubmenu::SortBy)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();
        assert_eq!(
            sort_actions,
            vec![
                (ContextMenuAction::SortByName, true),
                (ContextMenuAction::SortByModified, true),
                (ContextMenuAction::SortBySize, true),
                (ContextMenuAction::SortAscending, true),
                (ContextMenuAction::SortDescending, true),
                (ContextMenuAction::SortFoldersFirst, true),
                (ContextMenuAction::SortHiddenLast, true),
            ]
        );

        let view_actions = context_submenu_actions(ContextMenuSubmenu::ViewMode)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();
        assert_eq!(
            view_actions,
            vec![
                (ContextMenuAction::ViewCompact, true),
                (ContextMenuAction::ViewIcons, false),
                (ContextMenuAction::ViewDetails, false),
            ]
        );
    }

    #[test]
    fn places_blank_context_menu_offers_add_and_show_hidden_places() {
        let target = ContextMenuTarget::PlacesBlank {
            hidden_sections: false,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::AddPlace, true),
                (ContextMenuAction::ShowHiddenPlaces, false),
            ]
        );

        let target = ContextMenuTarget::PlacesBlank {
            hidden_sections: true,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::AddPlace, true),
                (ContextMenuAction::ShowHiddenPlaces, true),
            ]
        );
    }

    #[test]
    fn places_section_context_menu_offers_hide_section() {
        let target = ContextMenuTarget::PlaceSection { group: "Devices" };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![(ContextMenuAction::HidePlaceSection, true)]
        );
    }

    #[test]
    fn places_user_bookmark_context_menu_enables_edit_and_remove() {
        let target = ContextMenuTarget::Place {
            path: PathBuf::from("/tmp/fika-user-place"),
            trash_place: false,
            trash_has_items: false,
            editable: true,
            removable: true,
        };
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::EditPlace, true),
                (ContextMenuAction::RemovePlace, true),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn context_menu_actions_offer_new_pane_only_for_directories() {
        let dir_target = context_item_target("/tmp", true, 1);
        let file_target = context_item_target("/tmp/readme.txt", false, 1);

        assert!(
            context_menu_actions(&dir_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::OpenInNewPane)
        );
        assert!(
            !context_menu_actions(&file_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::OpenInNewPane)
        );
        assert!(
            context_menu_actions(&file_target, false)
                .iter()
                .any(|item| item.action == ContextMenuAction::CopyLocation)
        );
    }

    #[test]
    fn context_menu_actions_offer_paste_only_for_single_directory_targets() {
        let dir_target = context_item_target("/tmp", true, 1);
        let file_target = context_item_target("/tmp/readme.txt", false, 1);

        assert_eq!(
            context_menu_actions(&dir_target, true)
                .iter()
                .find(|item| item.action == ContextMenuAction::Paste)
                .map(|item| item.enabled),
            Some(true)
        );
        assert!(
            !context_menu_actions(&file_target, true)
                .iter()
                .any(|item| item.action == ContextMenuAction::Paste)
        );
    }

    #[test]
    fn context_menu_actions_use_batch_actions_for_multi_selection() {
        let target = context_item_target("/tmp/readme.txt", false, 3);
        let actions = context_menu_actions(&target, false)
            .into_iter()
            .map(|item| item.action)
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                ContextMenuAction::Copy,
                ContextMenuAction::Cut,
                ContextMenuAction::Trash,
                ContextMenuAction::Properties
            ]
        );
    }

    #[test]
    fn context_menu_actions_use_trash_view_actions() {
        let blank = ContextMenuTarget::Blank {
            trash_view: true,
            trash_has_items: false,
        };
        let blank_actions = context_menu_actions(&blank, false);
        assert_eq!(
            blank_actions
                .iter()
                .find(|item| item.action == ContextMenuAction::EmptyTrash)
                .map(|item| item.enabled),
            Some(false)
        );
        assert!(
            !blank_actions
                .iter()
                .any(|item| item.action == ContextMenuAction::CreateFolder)
        );

        let item = ContextMenuTarget::Item {
            path: PathBuf::from("/tmp/fika-trash-item"),
            is_dir: false,
            selection_count: 2,
            trash_view: true,
            trash_can_restore: true,
        };
        let item_actions = context_menu_actions(&item, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            item_actions,
            vec![
                (ContextMenuAction::RestoreFromTrash, true),
                (ContextMenuAction::Copy, true),
                (ContextMenuAction::DeletePermanently, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn context_menu_actions_use_place_actions_for_trash_place() {
        let empty_trash = context_place_target(file_ops::trash_files_dir(), true, false);
        let empty_actions = context_menu_actions(&empty_trash, false)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            empty_actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::EmptyTrash, false),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );

        let non_empty_trash = context_place_target(file_ops::trash_files_dir(), true, true);
        assert_eq!(
            context_menu_actions(&non_empty_trash, false)
                .iter()
                .find(|item| item.action == ContextMenuAction::EmptyTrash)
                .map(|item| item.enabled),
            Some(true)
        );
        assert!(
            !context_menu_actions(&non_empty_trash, true)
                .iter()
                .any(|item| matches!(
                    item.action,
                    ContextMenuAction::CreateFolder
                        | ContextMenuAction::Paste
                        | ContextMenuAction::Trash
                ))
        );
    }

    #[test]
    fn context_menu_actions_use_basic_actions_for_normal_places() {
        let home = context_place_target(PathBuf::from("/home/yk"), false, false);
        let actions = context_menu_actions(&home, true)
            .into_iter()
            .map(|item| (item.action, item.enabled))
            .collect::<Vec<_>>();

        assert_eq!(
            actions,
            vec![
                (ContextMenuAction::Open, true),
                (ContextMenuAction::OpenInNewPane, true),
                (ContextMenuAction::EditPlace, false),
                (ContextMenuAction::RemovePlace, false),
                (ContextMenuAction::CopyLocation, true),
                (ContextMenuAction::Properties, true),
            ]
        );
    }

    #[test]
    fn build_places_loads_persistent_user_bookmarks_before_grouped_devices() {
        let root = test_dir("places-load");
        let bookmark = root.join("bookmark");
        std::fs::create_dir_all(&bookmark).unwrap();
        let path = root.join("user-places.xbel");
        fika_core::save_user_places(
            &path,
            &[
                UserPlace::new("Bookmark".to_string(), bookmark.clone()),
                UserPlace::new("Duplicate Root".to_string(), PathBuf::from("/")),
            ],
        )
        .unwrap();

        let places = build_places(&path);
        let bookmark_index = places
            .iter()
            .position(|place| place.path == bookmark)
            .expect("persistent bookmark should be loaded");
        let root_index = places
            .iter()
            .position(|place| place.path == PathBuf::from("/"))
            .expect("root device place should exist");

        assert!(bookmark_index < root_index);
        assert_eq!(places[bookmark_index].label, "Bookmark");
        assert_eq!(places[bookmark_index].marker, "B");
        assert!(places[bookmark_index].editable);
        assert!(places[bookmark_index].removable);
        assert_eq!(places[root_index].label, "Root");
        assert!(!places[root_index].editable);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn add_place_inserts_user_bookmark_before_grouped_entries() {
        let current = test_dir("place-add-current");
        std::fs::create_dir_all(&current).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: home_dir(),
                editable: false,
                removable: false,
            },
            PlaceEntry {
                group: "Devices",
                marker: "/",
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                editable: false,
                removable: false,
            },
        ];

        app.start_add_place(pane_id);
        assert_eq!(
            app.place_draft.as_ref().map(|draft| draft.path.as_str()),
            Some(current_arg.as_str())
        );
        app.commit_place_draft();

        assert_eq!(app.places.len(), 3);
        assert_eq!(app.places[1].path, current);
        assert_eq!(app.places[1].group, "");
        assert_eq!(app.places[1].marker, "B");
        assert_eq!(
            app.places[1].label,
            default_place_label(&app.places[1].path)
        );
        assert!(app.places[1].editable);
        assert!(app.places[1].removable);
        assert_eq!(app.places[2].group, "Devices");
        assert!(app.place_draft.is_none());
        assert!(
            app.status_message_for_pane(pane_id)
                .starts_with("Added place ")
        );
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![UserPlace::new(
                default_place_label(&current),
                current.clone()
            )])
        );

        let _ = std::fs::remove_dir_all(current);
    }

    #[test]
    fn edit_place_updates_only_editable_user_bookmarks_and_rejects_duplicates() {
        let current = test_dir("place-edit-current");
        let original = test_dir("place-edit-original");
        let duplicate = test_dir("place-edit-duplicate");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&original).unwrap();
        std::fs::create_dir_all(&duplicate).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Original".to_string(),
                path: original.clone(),
                editable: true,
                removable: true,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "Duplicate".to_string(),
                path: duplicate.clone(),
                editable: true,
                removable: true,
            },
        ];

        app.start_edit_place(pane_id, duplicate.clone());
        if let Some(draft) = &mut app.place_draft {
            draft.label = "Rejected".to_string();
            draft.path = original.display().to_string();
        }
        app.commit_place_draft();
        assert_eq!(app.status_message_for_pane(pane_id), "Place already exists");
        assert_eq!(app.places[1].label, "Duplicate");
        assert_eq!(app.places[1].path, duplicate);

        app.start_edit_place(pane_id, original.clone());
        if let Some(draft) = &mut app.place_draft {
            draft.label = "Edited".to_string();
            draft.path = current.display().to_string();
        }
        app.commit_place_draft();

        assert_eq!(app.places[0].label, "Edited");
        assert_eq!(app.places[0].path, current);
        assert!(app.places[0].editable);
        assert!(app.places[0].removable);
        assert_eq!(app.status_message_for_pane(pane_id), "Updated place Edited");
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(vec![
                UserPlace::new("Edited".to_string(), current.clone()),
                UserPlace::new("Duplicate".to_string(), duplicate.clone()),
            ])
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(original);
        let _ = std::fs::remove_dir_all(duplicate);
    }

    #[test]
    fn remove_place_only_removes_removable_user_bookmarks() {
        let current = test_dir("place-remove-current");
        let user = test_dir("place-remove-user");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        let current_arg = current.display().to_string();
        let mut app = test_app_with_entries(&current_arg, &[]);
        let pane_id = app.panes.focused().unwrap();
        app.places = vec![
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: current.clone(),
                editable: false,
                removable: false,
            },
            PlaceEntry {
                group: "",
                marker: "B",
                label: "User".to_string(),
                path: user.clone(),
                editable: true,
                removable: true,
            },
        ];
        app.place_draft = Some(PlaceDraft {
            pane_id,
            editing_path: Some(user.clone()),
            focus: PlaceDraftField::Label,
            label: "User".to_string(),
            path: user.display().to_string(),
        });

        app.remove_place(pane_id, &current);
        assert_eq!(
            app.status_message_for_pane(pane_id),
            "Place cannot be removed"
        );
        assert_eq!(app.places.len(), 2);

        app.remove_place(pane_id, &user);
        assert_eq!(app.places.len(), 1);
        assert_eq!(app.places[0].label, "Home");
        assert_eq!(app.status_message_for_pane(pane_id), "Removed place User");
        assert!(app.place_draft.is_none());
        assert_eq!(
            fika_core::load_user_places(&app.user_places_path),
            Ok(Vec::new())
        );

        let _ = std::fs::remove_dir_all(current);
        let _ = std::fs::remove_dir_all(user);
    }

    #[test]
    fn properties_for_path_reports_file_metadata_without_recursive_work() {
        let temp = test_dir("properties-file");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("note.txt");
        std::fs::write(&file, "properties").unwrap();

        let dialog = properties_for_path(&file);

        assert_eq!(dialog.title, "Properties - note.txt");
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value == "File")
        );
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Size" && row.value == "10 B")
        );
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn properties_for_selection_summarizes_selected_items() {
        let temp = test_dir("properties-selection");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("note.txt");
        let folder = temp.join("folder");
        std::fs::write(&file, "abc").unwrap();
        std::fs::create_dir_all(&folder).unwrap();

        let dialog = properties_for_selection(&[file, folder]);

        assert_eq!(dialog.title, "Properties - 2 items");
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value.contains("1 folder"))
        );
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value.contains("1 file"))
        );
        assert!(
            dialog
                .rows
                .iter()
                .any(|row| row.label == "Size" && row.value == "3 B")
        );
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn select_all_keystroke_uses_secondary_modifier() {
        let mut keystroke = gpui::Keystroke::parse("secondary-a").unwrap();
        assert_eq!(pane_shortcut(&keystroke), Some(PaneShortcut::SelectAll));

        keystroke.modifiers.shift = true;
        assert_eq!(pane_shortcut(&keystroke), None);
    }

    #[test]
    fn pane_shortcuts_classify_navigation_and_selection_keys() {
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("escape").unwrap()),
            Some(PaneShortcut::ClearSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f5").unwrap()),
            Some(PaneShortcut::Refresh)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f3").unwrap()),
            Some(PaneShortcut::SplitPane)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f2").unwrap()),
            Some(PaneShortcut::RenameSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("f6").unwrap()),
            Some(PaneShortcut::EditLocation)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("up").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: false,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("right").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: false,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("shift-left").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Previous,
                extend: true,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("shift-down").unwrap()),
            Some(PaneShortcut::MoveSelection {
                direction: SelectionMove::Next,
                extend: true,
            })
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("backspace").unwrap()),
            Some(PaneShortcut::GoParent)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("alt-left").unwrap()),
            Some(PaneShortcut::GoBack)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("alt-right").unwrap()),
            Some(PaneShortcut::GoForward)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("alt-d").unwrap()),
            Some(PaneShortcut::EditLocation)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("delete").unwrap()),
            Some(PaneShortcut::TrashSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-z").unwrap()),
            Some(PaneShortcut::Undo)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-c").unwrap()),
            Some(PaneShortcut::CopySelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("/").unwrap()),
            Some(PaneShortcut::ShowFilter)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-i").unwrap()),
            Some(PaneShortcut::ShowFilter)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-=").unwrap()),
            Some(PaneShortcut::Zoom(ZoomChange::In))
        );
        let mut shifted_plus = gpui::Keystroke::parse("secondary-shift-=").unwrap();
        shifted_plus.key_char = Some("+".to_string());
        assert_eq!(
            pane_shortcut(&shifted_plus),
            Some(PaneShortcut::Zoom(ZoomChange::In))
        );
        let mut zoom_out = gpui::Keystroke::parse("secondary-x").unwrap();
        zoom_out.key = "-".to_string();
        assert_eq!(
            pane_shortcut(&zoom_out),
            Some(PaneShortcut::Zoom(ZoomChange::Out))
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-0").unwrap()),
            Some(PaneShortcut::Zoom(ZoomChange::Reset))
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-l").unwrap()),
            Some(PaneShortcut::EditLocation)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-v").unwrap()),
            Some(PaneShortcut::PasteIntoPane)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-w").unwrap()),
            Some(PaneShortcut::ClosePane)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-x").unwrap()),
            Some(PaneShortcut::CutSelection)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("secondary-shift-n").unwrap()),
            Some(PaneShortcut::CreateFolder)
        );
        assert_eq!(
            pane_shortcut(&gpui::Keystroke::parse("shift-f5").unwrap()),
            None
        );
    }

    #[test]
    fn compact_layout_options_derive_size_from_zoom_level() {
        let default_options = ui::file_grid::compact_layout_options(&ViewState::default(), 0.0);
        assert_eq!(default_options.icon_size, 48.0);
        assert_eq!(default_options.item_width, 168.0);
        assert_eq!(default_options.item_height, 76.0);

        let zoomed_options = ui::file_grid::compact_layout_options(
            &ViewState {
                zoom_level: fika_core::MAX_ZOOM_LEVEL,
                ..ViewState::default()
            },
            0.0,
        );
        assert_eq!(zoomed_options.icon_size, 256.0);
        assert_eq!(zoomed_options.item_width, 376.0);
        assert_eq!(zoomed_options.item_height, 284.0);
    }

    #[test]
    fn status_bar_zoom_track_maps_drag_position_to_level() {
        assert_eq!(
            ui::status_bar::zoom_level_for_track_x(
                -10.0,
                160.0,
                fika_core::MIN_ZOOM_LEVEL,
                fika_core::MAX_ZOOM_LEVEL
            ),
            fika_core::MIN_ZOOM_LEVEL
        );
        assert_eq!(
            ui::status_bar::zoom_level_for_track_x(
                80.0,
                160.0,
                fika_core::MIN_ZOOM_LEVEL,
                fika_core::MAX_ZOOM_LEVEL
            ),
            8
        );
        assert_eq!(
            ui::status_bar::zoom_level_for_track_x(
                200.0,
                160.0,
                fika_core::MIN_ZOOM_LEVEL,
                fika_core::MAX_ZOOM_LEVEL
            ),
            fika_core::MAX_ZOOM_LEVEL
        );
    }

    #[test]
    fn status_messages_are_pane_local() {
        let mut app = test_app_with_entries("/tmp/fika-status-a", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        app.panes.focus(second);

        app.set_pane_status(first, "First pane");

        assert_eq!(app.status_message_for_pane(first), "First pane");
        assert_eq!(app.status_message_for_pane(second), "Ready");

        app.set_pane_status(second, "Second pane");

        assert_eq!(app.status_message_for_pane(first), "First pane");
        assert_eq!(app.status_message_for_pane(second), "Second pane");
    }

    #[test]
    fn zoom_status_updates_only_target_pane() {
        let mut app = test_app_with_entries("/tmp/fika-status-zoom", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        app.panes.focus(second);
        let next_level = fika_core::DEFAULT_ZOOM_LEVEL + 1;

        app.set_zoom_level(first, next_level);

        assert_eq!(
            app.status_message_for_pane(first),
            format!(
                "Zoom level {next_level} ({} px)",
                fika_core::icon_size_for_zoom_level(next_level) as i32
            )
        );
        assert_eq!(app.status_message_for_pane(second), "Ready");
    }

    #[test]
    fn operation_progress_snapshot_is_pane_local() {
        let mut app = test_app_with_entries("/tmp/fika-status-progress", &["one.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        app.begin_pane_operation(first, "Copying");
        let (_cancel, progress) = app.start_transfer_progress(first, "Copy".to_string());
        {
            let mut progress = progress.lock().unwrap();
            progress.bytes_done = 40;
            progress.bytes_total = 100;
        }
        let now = app.operation_progress.as_ref().unwrap().started_at + PROGRESS_DISPLAY_DELAY;

        let snapshot = app
            .operation_progress_snapshot_for_pane(first, now)
            .unwrap();

        assert_eq!(app.status_message_for_pane(first), "Copying");
        assert_eq!(snapshot.label, "Copy");
        assert_eq!(snapshot.percent, Some(40));
        assert!(snapshot.cancellable);
        assert!(
            app.operation_progress_snapshot_for_pane(second, now)
                .is_none()
        );
    }

    #[test]
    fn rename_input_action_classifies_controls_and_text() {
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            RenameInputAction::Cancel
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            RenameInputAction::Commit
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            RenameInputAction::Backspace
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("a->a").unwrap()),
            RenameInputAction::Insert("a".to_string())
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            RenameInputAction::Insert("A".to_string())
        );
        assert_eq!(
            rename_input_action(&gpui::Keystroke::parse("secondary-a").unwrap()),
            RenameInputAction::Ignore
        );
    }

    #[test]
    fn location_input_action_classifies_controls_completion_and_text() {
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            LocationInputAction::Cancel
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            LocationInputAction::Commit
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("tab").unwrap()),
            LocationInputAction::Complete
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            LocationInputAction::Backspace
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("/->/").unwrap()),
            LocationInputAction::Insert("/".to_string())
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            LocationInputAction::Insert("A".to_string())
        );
        assert_eq!(
            location_input_action(&gpui::Keystroke::parse("secondary-l").unwrap()),
            LocationInputAction::Ignore
        );
    }

    #[test]
    fn place_input_action_classifies_controls_field_switching_and_text() {
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            PlaceInputAction::Cancel
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            PlaceInputAction::Commit
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("tab").unwrap()),
            PlaceInputAction::NextField
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            PlaceInputAction::Backspace
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("/->/").unwrap()),
            PlaceInputAction::Insert("/".to_string())
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            PlaceInputAction::Insert("A".to_string())
        );
        assert_eq!(
            place_input_action(&gpui::Keystroke::parse("secondary-a").unwrap()),
            PlaceInputAction::Ignore
        );
    }

    #[test]
    fn filter_input_action_classifies_controls_navigation_and_text() {
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("escape").unwrap()),
            FilterInputAction::Cancel
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("enter").unwrap()),
            FilterInputAction::FocusView
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("down").unwrap()),
            FilterInputAction::PassToView
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("pageup").unwrap()),
            FilterInputAction::PassToView
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("backspace").unwrap()),
            FilterInputAction::Backspace
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("a->a").unwrap()),
            FilterInputAction::Insert("a".to_string())
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("shift-a->A").unwrap()),
            FilterInputAction::Insert("A".to_string())
        );
        assert_eq!(
            filter_input_action(&gpui::Keystroke::parse("secondary-i").unwrap()),
            FilterInputAction::Ignore
        );
    }

    #[test]
    fn filter_projection_is_pane_local_and_navigation_clears_query() {
        let mut app = test_app_with_entries("/tmp/fika-filter-a", &["alpha.rs", "beta.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();

        app.set_filter_query(first, "*.rs".to_string());
        let first_filtered = app.filtered_model_for_pane(first).unwrap().0;
        assert_eq!(first_filtered.as_slice(), &[0]);
        assert!(app.filtered_model_for_pane(second).is_none());
        assert!(!app.panes.can_go_back(first));

        let next_dir = PathBuf::from("/tmp/fika-filter-b");
        app.load_pane(first, next_dir.clone());
        let first_filter = app.pane_filters.get(&first).unwrap();
        assert!(first_filter.query.is_empty());
        assert!(!first_filter.focused);
        assert!(app.filtered_models.get(&first).is_none());
        assert!(app.panes.can_go_back(first));
        assert_eq!(
            app.panes.pane(first).map(|pane| pane.current_dir.as_path()),
            Some(next_dir.as_path())
        );
    }

    #[test]
    fn filter_projection_rebuilds_after_model_signal() {
        let mut app = test_app_with_entries("/tmp/fika-filter-model", &["alpha.rs", "beta.txt"]);
        let pane_id = app.panes.focused().unwrap();
        app.set_filter_query(pane_id, "*.rs".to_string());
        assert!(app.filtered_model_for_pane(pane_id).is_some());
        assert!(app.filtered_models.contains_key(&pane_id));

        let generation = app.panes.pane(pane_id).unwrap().generation;
        app.apply_event(DirectoryListerEvent::ItemsAdded {
            pane_id,
            generation,
            request_serial: fika_core::RequestSerial(0),
            path: PathBuf::from("/tmp/fika-filter-model"),
            entries: vec![test_entry("gamma.rs")],
        });

        assert!(app.filtered_models.get(&pane_id).is_none());
        let filtered = app.filtered_model_for_pane(pane_id).unwrap().0;
        assert_eq!(filtered.as_slice(), &[0, 2]);
    }

    #[test]
    fn pane_sort_updates_only_target_pane_and_drops_target_layout_caches() {
        let mut app = test_app_with_entries("/tmp/fika-sort-a", &["alpha.rs", "beta.txt"]);
        let first = app.panes.focused().unwrap();
        let second = app.panes.split(first).unwrap();
        let first_alpha = PathBuf::from("/tmp/fika-sort-a/alpha.rs");

        app.select_only(first, first_alpha.clone());
        app.set_filter_query(first, "*.rs".to_string());
        app.set_filter_query(second, "*.rs".to_string());
        assert!(app.filtered_model_for_pane(first).is_some());
        assert!(app.filtered_model_for_pane(second).is_some());
        assert!(app.status_summary_for_pane(first).is_some());
        assert!(app.status_summary_for_pane(second).is_some());

        let first_ids = app
            .panes
            .pane(first)
            .unwrap()
            .model
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let second_ids = app
            .panes
            .pane(second)
            .unwrap()
            .model
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        app.visible_item_slots
            .entry(first)
            .or_default()
            .slots_for_items(first_ids);
        app.visible_item_slots
            .entry(second)
            .or_default()
            .slots_for_items(second_ids);
        app.compact_column_widths
            .insert(first, CompactColumnWidthCache::default());
        app.compact_column_widths
            .insert(second, CompactColumnWidthCache::default());
        app.panes.pane_mut(first).unwrap().view.scroll_x = 64.0;
        app.panes.pane_mut(second).unwrap().view.scroll_x = 32.0;

        app.set_pane_sort_order(first, SortOrder::Descending);

        let pane_names = |app: &FikaApp, pane_id: PaneId| {
            app.panes
                .pane(pane_id)
                .unwrap()
                .model
                .entries()
                .iter()
                .map(|entry| entry.name.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(pane_names(&app, first), vec!["beta.txt", "alpha.rs"]);
        assert_eq!(pane_names(&app, second), vec!["alpha.rs", "beta.txt"]);
        assert!(app.panes.is_selected(first, &first_alpha));
        assert_eq!(app.panes.pane(first).unwrap().view.scroll_x, 0.0);
        assert_eq!(app.panes.pane(second).unwrap().view.scroll_x, 32.0);
        assert!(!app.visible_item_slots.contains_key(&first));
        assert!(app.visible_item_slots.contains_key(&second));
        assert!(!app.compact_column_widths.contains_key(&first));
        assert!(app.compact_column_widths.contains_key(&second));
        assert!(!app.filtered_models.contains_key(&first));
        assert!(app.filtered_models.contains_key(&second));
        assert!(!app.status_summaries.contains_key(&first));
        assert!(app.status_summaries.contains_key(&second));
        assert_eq!(
            app.status_message_for_pane(first),
            "Sorted by Name (Descending)"
        );
        assert_eq!(app.status_message_for_pane(second), "Filtering");
    }

    #[test]
    fn rename_item_result_renames_item_and_records_affected_dir() {
        let temp = test_dir("rename-item");
        std::fs::create_dir_all(&temp).unwrap();
        let original = temp.join("old.txt");
        let renamed = temp.join("new.txt");
        std::fs::write(&original, "rename").unwrap();

        let result = rename_item_result(PaneId(11), original.clone(), "new.txt".to_string());
        let renamed_path = result.result.unwrap();

        assert_eq!(result.pane_id, PaneId(11));
        assert_eq!(result.original_path, original.clone());
        assert_eq!(result.affected_dirs, vec![temp.clone()]);
        assert_eq!(renamed_path, renamed);
        assert!(!original.exists());
        assert_eq!(std::fs::read_to_string(&renamed_path).unwrap(), "rename");
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn create_item_result_creates_default_folder_and_records_affected_dir() {
        let temp = test_dir("create-folder");
        std::fs::create_dir_all(&temp).unwrap();

        let result = create_item_result(PaneId(5), temp.clone(), CreatedItemKind::Folder);
        let created = result.result.unwrap();

        assert_eq!(result.pane_id, PaneId(5));
        assert_eq!(result.kind, CreatedItemKind::Folder);
        assert_eq!(result.affected_dirs, vec![temp.clone()]);
        assert_eq!(created.file_name().unwrap(), "New Folder");
        assert!(created.is_dir());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn create_item_result_uses_keep_both_name_for_default_file() {
        let temp = test_dir("create-file");
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(temp.join("New File.txt"), "occupied").unwrap();

        let result = create_item_result(PaneId(6), temp.clone(), CreatedItemKind::File);
        let created = result.result.unwrap();

        assert_eq!(result.kind, CreatedItemKind::File);
        assert_eq!(result.affected_dirs, vec![temp.clone()]);
        assert_eq!(created.file_name().unwrap(), "New File copy.txt");
        assert!(created.is_file());
        assert!(temp.join("New File.txt").exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn status_summary_reports_current_model_without_selection() {
        let entries = vec![
            status_entry(1, "folder", true, 0),
            status_entry(2, "large.bin", false, 1536),
            status_entry(3, "small.txt", false, 512),
        ];

        assert_eq!(
            status_summary_for_model(&entries, &fika_core::SelectionState::default()),
            "1 folder, 2 files (2.0 KB)"
        );
    }

    #[test]
    fn status_summary_reports_selected_items_without_path_expansion() {
        let entries = vec![
            status_entry(1, "folder", true, 0),
            status_entry(2, "large.bin", false, 1536),
            status_entry(3, "small.txt", false, 512),
        ];
        let mut selection = fika_core::SelectionState::default();
        selection.select_all(Some(fika_core::ItemId(1)));
        assert_eq!(selection.toggle(fika_core::ItemId(2)), false);

        assert_eq!(
            status_summary_for_model(&entries, &selection),
            "1 folder selected, 1 file selected (512 B)"
        );
    }

    #[test]
    fn space_info_snapshot_formats_free_space_and_used_percent() {
        let snapshot = space_info_snapshot(4096, 1024).unwrap();

        assert_eq!(snapshot.free_label, "1.0 KB free");
        assert_eq!(
            snapshot.detail_label,
            "1.0 KB free out of 4.0 KB (75% used)"
        );
        assert_eq!(snapshot.used_percent, 75);
        assert_eq!(
            parse_df_space_output("1B-blocks Avail\n4096 1024\n"),
            Some(snapshot)
        );
    }

    #[test]
    fn progress_percent_handles_unknown_and_complete_totals() {
        assert_eq!(progress_percent(0, 0), None);
        assert_eq!(progress_percent(50, 100), Some(50));
        assert_eq!(progress_percent(128, 128), Some(100));
        assert_eq!(progress_percent(256, 128), Some(100));
    }

    #[test]
    fn progress_delay_matches_dolphin_delayed_progress_bar() {
        let started = Instant::now();

        assert!(!progress_delay_elapsed(
            started,
            started + PROGRESS_DISPLAY_DELAY - Duration::from_millis(1)
        ));
        assert!(progress_delay_elapsed(
            started,
            started + PROGRESS_DISPLAY_DELAY
        ));
    }

    #[test]
    fn clipboard_state_round_trips_file_clipboard_item_metadata() {
        let paths = vec![
            PathBuf::from("/tmp/fika-copy-a.txt"),
            PathBuf::from("/tmp/fika-copy-b.txt"),
        ];
        let clipboard = ClipboardState::files(ClipboardMode::Cut, paths.clone());
        let item = clipboard.to_clipboard_item();

        assert_eq!(
            ClipboardState::from_clipboard_item(&item),
            Some(ClipboardState::files(ClipboardMode::Cut, paths))
        );
    }

    #[test]
    fn clipboard_state_imports_uri_list_text_and_plain_text() {
        let uri_list =
            ClipboardItem::new_string("copy\nfile:///tmp/fika%20clipboard.txt\n".to_string());
        assert_eq!(
            ClipboardState::from_clipboard_item(&uri_list),
            Some(ClipboardState::files(
                ClipboardMode::Copy,
                vec![PathBuf::from("/tmp/fika clipboard.txt")]
            ))
        );

        let plain = ClipboardItem::new_string("hello from clipboard".to_string());
        assert_eq!(
            ClipboardState::from_clipboard_item(&plain),
            ClipboardState::text("hello from clipboard".to_string())
        );
    }

    #[test]
    fn paste_clipboard_result_copies_item_and_records_transfer_undo() {
        let temp = test_dir("paste-copy");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        std::fs::write(&source, "copy").unwrap();

        let result = paste_clipboard_result(
            PaneId(7),
            target_dir.clone(),
            ClipboardState::files(ClipboardMode::Copy, vec![source.clone()]),
            None,
            None,
        );

        let destination = target_dir.join("note.txt");
        assert_eq!(result.pane_id, PaneId(7));
        assert_eq!(result.mode, ClipboardMode::Copy);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(result.affected_dirs, vec![target_dir.clone()]);
        assert_eq!(
            result.undo_items,
            vec![TransferUndoItem {
                operation: "copy".to_string(),
                original_source: source.clone(),
                destination: destination.clone(),
                overwritten_backup: None,
            }]
        );
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "copy");
        assert!(source.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn paste_clipboard_result_writes_plain_text_file_and_records_create_undo() {
        let temp = test_dir("paste-text");
        std::fs::create_dir_all(&temp).unwrap();

        let result = paste_clipboard_result(
            PaneId(15),
            temp.clone(),
            ClipboardState::text("plain text".to_string()).unwrap(),
            None,
            None,
        );

        let destination = temp.join("Pasted Text.txt");
        assert_eq!(result.pane_id, PaneId(15));
        assert_eq!(result.mode, ClipboardMode::Copy);
        assert_eq!(result.label, "Paste");
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(result.affected_dirs, vec![temp.clone()]);
        assert!(result.undo_items.is_empty());
        assert_eq!(
            result.created_items,
            vec![CreateUndoItem {
                path: destination.clone(),
                kind: CreatedItemKind::File,
            }]
        );
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "plain text");
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn paste_clipboard_result_updates_shared_transfer_progress() {
        let temp = test_dir("paste-progress");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.bin");
        std::fs::write(&source, vec![42_u8; 32 * 1024]).unwrap();
        let progress = Arc::new(Mutex::new(file_ops::TransferProgress::default()));

        let result = paste_clipboard_result(
            PaneId(13),
            target_dir,
            ClipboardState::files(ClipboardMode::Copy, vec![source]),
            None,
            Some(Arc::clone(&progress)),
        );

        assert_eq!(result.success_count, 1);
        let progress = *progress.lock().unwrap();
        assert_eq!(progress.bytes_total, 32 * 1024);
        assert_eq!(progress.bytes_done, 32 * 1024);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn paste_clipboard_result_honors_cancel_flag_before_transfer() {
        let temp = test_dir("paste-cancel");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.bin");
        std::fs::write(&source, "cancel").unwrap();
        let cancel = Arc::new(AtomicBool::new(true));

        let result = paste_clipboard_result(
            PaneId(14),
            target_dir.clone(),
            ClipboardState::files(ClipboardMode::Copy, vec![source]),
            Some(cancel),
            None,
        );

        assert_eq!(result.success_count, 0);
        assert_eq!(result.failure_count, 1);
        assert!(std::fs::read_dir(&target_dir).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn trash_view_operation_result_restores_items_and_marks_original_dir() {
        let temp = test_dir("trash-restore");
        std::fs::create_dir_all(&temp).unwrap();
        let unique_name = format!(
            "restore-{}.txt",
            temp.file_name().unwrap().to_string_lossy()
        );
        let original = temp.join(unique_name);
        std::fs::write(&original, "restore").unwrap();
        let trashed = file_ops::trash_paths(std::slice::from_ref(&original));
        assert!(trashed.failures.is_empty());
        let trash_path = trashed.successes[0].trash_path.clone();
        assert!(!original.exists());

        let result =
            trash_view_operation_result(PaneId(16), TrashViewOperation::Restore, vec![trash_path]);

        assert_eq!(result.pane_id, PaneId(16));
        assert_eq!(result.operation, TrashViewOperation::Restore);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(
            result.affected_dirs,
            vec![file_ops::trash_files_dir(), temp.clone()]
        );
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "restore");
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn trash_view_operation_result_deletes_items_permanently() {
        let temp = test_dir("trash-delete-permanently");
        std::fs::create_dir_all(&temp).unwrap();
        let original = temp.join("delete.txt");
        std::fs::write(&original, "delete").unwrap();
        let trashed = file_ops::trash_paths(std::slice::from_ref(&original));
        assert!(trashed.failures.is_empty());
        let trash_path = trashed.successes[0].trash_path.clone();
        assert!(!original.exists());

        let result = trash_view_operation_result(
            PaneId(17),
            TrashViewOperation::DeletePermanently,
            vec![trash_path.clone()],
        );

        assert_eq!(result.pane_id, PaneId(17));
        assert_eq!(result.operation, TrashViewOperation::DeletePermanently);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(result.affected_dirs, vec![file_ops::trash_files_dir()]);
        assert!(!trash_path.exists());
        assert!(!original.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn paste_clipboard_result_moves_item_and_marks_both_directories() {
        let temp = test_dir("paste-move");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        std::fs::write(&source, "move").unwrap();

        let result = paste_clipboard_result(
            PaneId(8),
            target_dir.clone(),
            ClipboardState::files(ClipboardMode::Cut, vec![source.clone()]),
            None,
            None,
        );

        let destination = target_dir.join("note.txt");
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(
            result.affected_dirs,
            vec![target_dir.clone(), source_dir.clone()]
        );
        assert_eq!(result.undo_items[0].operation, "move");
        assert_eq!(result.undo_items[0].original_source, source);
        assert_eq!(result.undo_items[0].destination, destination.clone());
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "move");
        assert!(!source_dir.join("note.txt").exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_record_result_restores_transfer_payload() {
        let temp = test_dir("undo-transfer");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        let destination = target_dir.join("note.txt");
        std::fs::write(&source, "undo").unwrap();

        let paste = paste_clipboard_result(
            PaneId(9),
            target_dir,
            ClipboardState::files(ClipboardMode::Cut, vec![source.clone()]),
            None,
            None,
        );
        assert_eq!(paste.success_count, 1);
        assert!(destination.exists());
        assert!(!source.exists());

        let undo = undo_record_result(UndoRecord {
            serial: fika_core::UndoSerial(1),
            label: "Move".to_string(),
            affected_dirs: paste.affected_dirs,
            payload: UndoPayload::Transfer {
                items: paste.undo_items,
            },
        });

        assert_eq!(undo.result, Ok("restored 1 item(s)".to_string()));
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "undo");
        assert!(!destination.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_record_result_restores_rename_payload() {
        let temp = test_dir("undo-rename");
        std::fs::create_dir_all(&temp).unwrap();
        let original = temp.join("old.txt");
        std::fs::write(&original, "undo rename").unwrap();
        let rename = rename_item_result(PaneId(12), original.clone(), "new.txt".to_string());
        let renamed = rename.result.unwrap();
        assert!(renamed.exists());
        assert!(!original.exists());

        let undo = undo_record_result(UndoRecord {
            serial: fika_core::UndoSerial(1),
            label: "Rename".to_string(),
            affected_dirs: rename.affected_dirs,
            payload: UndoPayload::Rename {
                items: vec![RenameUndoItem {
                    original_path: original.clone(),
                    renamed_path: renamed.clone(),
                }],
            },
        });

        assert_eq!(undo.result, Ok("restored 1 item(s)".to_string()));
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "undo rename");
        assert!(!renamed.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_record_result_removes_created_payload() {
        let temp = test_dir("undo-create");
        std::fs::create_dir_all(&temp).unwrap();
        let create = create_item_result(PaneId(10), temp.clone(), CreatedItemKind::File);
        let created = create.result.unwrap();
        assert!(created.exists());

        let undo = undo_record_result(UndoRecord {
            serial: fika_core::UndoSerial(1),
            label: "Create File".to_string(),
            affected_dirs: create.affected_dirs,
            payload: UndoPayload::Create {
                items: vec![CreateUndoItem {
                    path: created.clone(),
                    kind: CreatedItemKind::File,
                }],
            },
        });

        assert_eq!(undo.result, Ok("removed 1 item(s)".to_string()));
        assert!(!created.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn affected_parent_dirs_are_stable_and_deduplicated() {
        let dirs = parent_dirs([
            PathBuf::from("/tmp/a/one.txt"),
            PathBuf::from("/tmp/a/two.txt"),
            PathBuf::from("/tmp/b/three.txt"),
        ]);

        assert_eq!(dirs, vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);
    }

    #[test]
    fn action_status_reports_mixed_file_operation_results() {
        assert_eq!(action_status("Moved", 2, 0), "Moved: 2 item(s)");
        assert_eq!(action_status("Moved", 0, 1), "Moved failed for 1 item(s)");
        assert_eq!(action_status("Moved", 2, 1), "Moved: 2 item(s), 1 failed");
    }

    #[test]
    fn visible_item_slot_pool_reuses_offscreen_slots() {
        let mut pool = VisibleItemSlotPool::default();
        let first = pool.slots_for_items([fika_core::ItemId(1), fika_core::ItemId(2)]);
        assert_eq!(first.len(), 2);

        let slot_for_one = first[&fika_core::ItemId(1)];
        let slot_for_two = first[&fika_core::ItemId(2)];
        let second = pool.slots_for_items([fika_core::ItemId(2), fika_core::ItemId(3)]);

        assert_eq!(second[&fika_core::ItemId(2)], slot_for_two);
        assert_eq!(second[&fika_core::ItemId(3)], slot_for_one);
        assert_eq!(pool.slot_by_item_id.len(), 2);
    }

    #[test]
    fn visible_item_slot_pool_caps_recycled_slots() {
        let mut pool = VisibleItemSlotPool::default();
        let visible = (1..=150).map(fika_core::ItemId).collect::<Vec<_>>();
        let first = pool.slots_for_items(visible);
        assert_eq!(first.len(), 150);

        let second = pool.slots_for_items(std::iter::empty::<fika_core::ItemId>());

        assert!(second.is_empty());
        assert_eq!(pool.free_slots.len(), VisibleItemSlotPool::MAX_FREE_SLOTS);
    }

    #[test]
    fn compact_column_width_cache_resolves_only_visible_columns() {
        let mut model = fika_core::DirectoryModel::for_directory(PathBuf::from("/tmp"));
        let entries = (0..120)
            .map(|index| {
                let name = if index == 80 {
                    format!(
                        "{index:03}-very-long-name-that-should-not-be-measured-until-scrolled.txt"
                    )
                } else {
                    format!("{index:03}.txt")
                };
                fika_core::Entry::new(fika_core::EntryData {
                    name: Arc::from(name.as_str()),
                    name_width_units: name.len() as u16,
                    size_bytes: 0,
                    modified_secs: None,
                    trash_original_path: None,
                    trash_deletion_time: None,
                    is_dir: false,
                })
            })
            .collect::<Vec<_>>();
        model.replace_listing(PathBuf::from("/tmp"), Arc::new(entries));

        let mut cache = CompactColumnWidthCache::default();
        let options = CompactLayoutOptions {
            viewport_width: 140.0,
            viewport_height: 128.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            scroll_x: 0.0,
            ..CompactLayoutOptions::default()
        };
        let rows_per_column = CompactLayout::rows_per_column_for_options(options);
        let metrics = cache.metrics_for_model(&model, rows_per_column, options);
        let column_count = model.len().div_ceil(rows_per_column);
        let resolved_count = cache.cached[0]
            .resolved_columns
            .iter()
            .filter(|resolved| **resolved)
            .count();

        assert!(resolved_count < column_count);
        let far_column = 80 / rows_per_column;
        assert_eq!(metrics.column_width(far_column), Some(options.item_width));

        let scrolled_options = CompactLayoutOptions {
            scroll_x: far_column as f32 * (options.item_width + options.gap),
            ..options
        };
        let metrics = cache.metrics_for_model(&model, rows_per_column, scrolled_options);

        assert!(
            metrics.column_width(far_column).unwrap() > options.item_width,
            "far column width should be resolved only after it enters the viewport"
        );
    }

    #[test]
    fn listing_requests_from_events_keeps_only_loading_events() {
        let first = listing_request(1, 1);
        let second = listing_request(2, 1);
        let events = vec![
            listing_started(&first),
            listing_completed(&first),
            listing_started(&second),
        ];

        assert_eq!(
            listing_requests_from_events(events.iter()),
            vec![first, second]
        );
    }

    #[test]
    fn listing_worker_state_keeps_latest_pending_request_per_pane() {
        let mut state = ListingWorkerState::default();
        let old_first = listing_request(1, 1);
        let second = listing_request(2, 1);
        let new_first = listing_request(1, 2);

        state.schedule(old_first);
        state.schedule(second.clone());
        state.schedule(new_first.clone());

        assert_eq!(
            state.pop_batch().map(|batch| batch.requests),
            Some(vec![second])
        );
        assert_eq!(
            state.pop_batch().map(|batch| batch.requests),
            Some(vec![new_first])
        );
        assert_eq!(state.pop_batch(), None);
    }

    #[test]
    fn listing_worker_state_batches_same_path_requests() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let different = listing_request_at(2, 1, "/tmp/fika-other-listing");
        let second = listing_request_at(3, 1, "/tmp/fika-shared-listing");

        state.schedule(first.clone());
        state.schedule(different.clone());
        state.schedule(second.clone());

        let shared_batch = state.pop_batch().unwrap();
        assert_eq!(shared_batch.path, PathBuf::from("/tmp/fika-shared-listing"));
        assert_eq!(shared_batch.requests, vec![first, second]);

        let different_batch = state.pop_batch().unwrap();
        assert_eq!(different_batch.requests, vec![different]);
        assert_eq!(state.pop_batch(), None);
    }

    #[test]
    fn retarget_listing_events_preserves_shared_listing_entries() {
        let source = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let target = listing_request_at(2, 7, "/tmp/fika-shared-listing");
        let entries = Arc::new(vec![fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from("shared.txt"),
            name_width_units: 10,
            size_bytes: 4,
            modified_secs: None,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })]);
        let events = vec![DirectoryListerEvent::ListingRefreshed {
            pane_id: source.pane_id,
            generation: source.generation,
            request_serial: source.request_serial,
            path: source.path.clone(),
            entries: Arc::clone(&entries),
        }];

        let retargeted = retarget_listing_events(&events, &target);

        let DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial,
            path,
            entries: retargeted_entries,
        } = &retargeted[0]
        else {
            panic!("expected retargeted listing");
        };
        assert_eq!(*pane_id, target.pane_id);
        assert_eq!(*generation, target.generation);
        assert_eq!(*request_serial, target.request_serial);
        assert_eq!(path, &target.path);
        assert!(Arc::ptr_eq(&entries, retargeted_entries));
    }

    #[test]
    fn listing_worker_state_drops_stale_results() {
        let mut state = ListingWorkerState::default();
        let old = listing_request(1, 1);
        let new = listing_request(1, 2);

        state.schedule(old.clone());
        let old_batch = listing_batch(vec![old.clone()]);
        let old_events = vec![listing_completed(&old)];
        assert!(state.publish_batch_if_current(&old_batch, &old_events));
        assert_eq!(state.results_by_pane.len(), 1);

        state.schedule(new.clone());
        assert!(state.results_by_pane.is_empty());
        assert!(!state.publish_batch_if_current(&old_batch, &old_events));
        assert!(state.drain_results().is_empty());

        let new_batch = listing_batch(vec![new.clone()]);
        let new_events = vec![listing_completed(&new)];
        assert!(state.publish_batch_if_current(&new_batch, &new_events));
        let results = state.drain_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0][0].request_serial(), fika_core::RequestSerial(2));
    }

    #[test]
    fn listing_worker_state_cancels_closed_pane_work() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let second = listing_request_at(2, 1, "/tmp/fika-shared-listing");
        state.schedule(first.clone());
        state.schedule(second.clone());

        let batch = listing_batch(vec![first.clone(), second.clone()]);
        let events = vec![listing_completed(&first)];
        assert!(state.publish_batch_if_current(&batch, &events));
        assert_eq!(state.results_by_pane.len(), 2);

        state.cancel_pane(first.pane_id);

        assert!(!state.latest_request_by_pane.contains_key(&first.pane_id));
        assert!(!state.results_by_pane.contains_key(&first.pane_id));
        assert!(
            state
                .pending
                .iter()
                .all(|pending| pending.pane_id != first.pane_id)
        );
        assert!(state.results_by_pane.contains_key(&second.pane_id));
    }

    #[test]
    fn listing_worker_cache_serves_load_with_shared_entries() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
        let second = listing_request_at(2, 2, "/tmp/fika-cached-listing");
        let entries = test_entries(&["cached.txt"]);
        let events = vec![
            listing_refreshed(&first, Arc::clone(&entries)),
            listing_completed(&first),
        ];

        state.schedule(first.clone());
        assert!(state.publish_batch_if_current(&listing_batch(vec![first]), &events));

        let cached = state.cached_events_for(&second).expect("cache miss");
        let DirectoryListerEvent::ListingRefreshed {
            pane_id,
            request_serial,
            entries: cached_entries,
            ..
        } = &cached[0]
        else {
            panic!("expected cached listing refresh");
        };
        assert_eq!(*pane_id, second.pane_id);
        assert_eq!(*request_serial, second.request_serial);
        assert!(Arc::ptr_eq(&entries, cached_entries));
        assert!(matches!(
            cached[1],
            DirectoryListerEvent::ListingCompleted { .. }
        ));
    }

    #[test]
    fn listing_worker_cache_hit_does_not_schedule_background_reload() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
        let second = listing_request_at(2, 2, "/tmp/fika-cached-listing");
        let entries = test_entries(&["cached.txt"]);
        let events = vec![
            listing_refreshed(&first, Arc::clone(&entries)),
            listing_completed(&first),
        ];

        state.schedule(first.clone());
        let first_batch = state
            .pop_batch()
            .expect("scheduled listing should be pending");
        assert_eq!(first_batch.requests, vec![first]);
        assert!(state.publish_batch_if_current(&first_batch, &events));

        let cached = state
            .schedule_or_cached(second.clone())
            .expect("fresh cache should serve request directly");

        assert_eq!(cached.len(), 2);
        assert!(state.pending.is_empty());
        assert_eq!(
            state.latest_request_by_pane.get(&second.pane_id),
            Some(&second.key())
        );
    }

    #[test]
    fn listing_worker_cache_ignores_reload_and_can_remove_directory() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
        let mut reload = listing_request_at(2, 2, "/tmp/fika-cached-listing");
        reload.mode = fika_core::LoadMode::Reload;
        let entries = test_entries(&["cached.txt"]);
        let events = vec![
            listing_refreshed(&first, Arc::clone(&entries)),
            listing_completed(&first),
        ];

        state.schedule(first.clone());
        assert!(state.publish_batch_if_current(&listing_batch(vec![first]), &events));

        assert!(state.cached_events_for(&reload).is_none());
        state.schedule(reload);
        let snapshot = state
            .cache
            .get(Path::new("/tmp/fika-cached-listing"))
            .expect("cache should retain stale payload");
        assert_eq!(snapshot.state(), fika_core::DirectoryCacheState::Stale);
        assert!(
            state
                .cached_events_for(&listing_request_at(3, 3, "/tmp/fika-cached-listing"))
                .is_none()
        );

        state.remove_cached_directory(Path::new("/tmp/fika-cached-listing"));
        assert!(
            state
                .cache
                .get(Path::new("/tmp/fika-cached-listing"))
                .is_none()
        );
    }

    #[test]
    fn listing_batch_cancelled_only_when_all_requests_are_stale() {
        let mut state = ListingWorkerState::default();
        let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
        let second = listing_request_at(2, 1, "/tmp/fika-shared-listing");
        state.schedule(first.clone());
        state.schedule(second.clone());
        let batch = listing_batch(vec![first.clone(), second.clone()]);
        let shared = Arc::new((Mutex::new(state), Condvar::new()));

        {
            let (lock, _) = &*shared;
            lock.lock()
                .expect("listing worker state poisoned")
                .cancel_pane(first.pane_id);
        }
        assert!(!listing_batch_cancelled(&shared, &batch));

        {
            let (lock, _) = &*shared;
            lock.lock()
                .expect("listing worker state poisoned")
                .cancel_pane(second.pane_id);
        }
        assert!(listing_batch_cancelled(&shared, &batch));
    }

    #[test]
    fn loading_state_tracks_current_request_and_ignores_stale_events() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/fika-loading"));
        let pane_id = controller.focused().unwrap();
        let start = controller.reload(pane_id).unwrap();
        let mut loading = HashMap::new();
        let now = Instant::now();

        update_loading_state_for_event(&mut loading, controller.pane(pane_id), &start, now);
        assert_eq!(
            loading.get(&pane_id).map(|state| state.key),
            Some(ListingRequestKey {
                generation: start.generation(),
                request_serial: start.request_serial(),
            })
        );

        let stale = DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation: start.generation(),
            request_serial: fika_core::RequestSerial(start.request_serial().0 + 1),
            path: start.path().to_path_buf(),
        };
        update_loading_state_for_event(&mut loading, controller.pane(pane_id), &stale, now);
        assert!(loading.contains_key(&pane_id));

        let completed = DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation: start.generation(),
            request_serial: start.request_serial(),
            path: start.path().to_path_buf(),
        };
        update_loading_state_for_event(&mut loading, controller.pane(pane_id), &completed, now);
        assert!(!loading.contains_key(&pane_id));
    }

    #[test]
    fn loading_state_rejects_stale_started_event_for_old_generation() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/fika-loading-a"));
        let pane_id = controller.focused().unwrap();
        let stale = controller.reload(pane_id).unwrap();
        controller.load(pane_id, PathBuf::from("/tmp/fika-loading-b"));
        let mut loading = HashMap::new();

        update_loading_state_for_event(
            &mut loading,
            controller.pane(pane_id),
            &stale,
            Instant::now(),
        );

        assert!(loading.is_empty());
    }

    fn listing_request(pane: u64, serial: u64) -> ListingRequest {
        listing_request_at(pane, serial, &format!("/tmp/fika-listing-{pane}"))
    }

    fn listing_request_at(pane: u64, serial: u64, path: &str) -> ListingRequest {
        ListingRequest {
            pane_id: PaneId(pane),
            generation: fika_core::Generation(1),
            request_serial: fika_core::RequestSerial(serial),
            path: PathBuf::from(path),
            mode: fika_core::LoadMode::Load,
        }
    }

    fn listing_batch(requests: Vec<ListingRequest>) -> ListingBatch {
        ListingBatch {
            path: requests[0].path.clone(),
            mode: requests[0].mode,
            requests,
        }
    }

    fn listing_started(request: &ListingRequest) -> DirectoryListerEvent {
        DirectoryListerEvent::LoadingStarted {
            pane_id: request.pane_id,
            generation: request.generation,
            request_serial: request.request_serial,
            path: request.path.clone(),
            mode: request.mode,
        }
    }

    fn listing_completed(request: &ListingRequest) -> DirectoryListerEvent {
        DirectoryListerEvent::ListingCompleted {
            pane_id: request.pane_id,
            generation: request.generation,
            request_serial: request.request_serial,
            path: request.path.clone(),
        }
    }

    fn listing_refreshed(
        request: &ListingRequest,
        entries: Arc<Vec<fika_core::Entry>>,
    ) -> DirectoryListerEvent {
        DirectoryListerEvent::ListingRefreshed {
            pane_id: request.pane_id,
            generation: request.generation,
            request_serial: request.request_serial,
            path: request.path.clone(),
            entries,
        }
    }

    fn test_app_with_entries(path: &str, names: &[&str]) -> FikaApp {
        let path = PathBuf::from(path);
        let mut panes = PaneController::new(path.clone());
        let pane_id = panes.focused().unwrap();
        panes
            .pane_mut(pane_id)
            .unwrap()
            .model
            .replace_listing(path, test_entries(names));
        FikaApp {
            panes,
            places: Vec::new(),
            hidden_place_sections: BTreeSet::new(),
            user_places_path: test_dir("user-places").join("user-places.xbel"),
            file_icons: FileIconCache::default(),
            space_info: SpaceInfoCache::default(),
            status_summaries: HashMap::new(),
            loading_panes: HashMap::new(),
            smooth_scrolls: HashMap::new(),
            scroll_drag_trackers: HashMap::new(),
            smooth_scroll_tick_running: false,
            viewport_origins: HashMap::new(),
            visible_item_slots: HashMap::new(),
            compact_column_widths: HashMap::new(),
            pane_filters: HashMap::new(),
            filtered_models: HashMap::new(),
            operations: OperationQueue::new(),
            clipboard: None,
            rename_draft: None,
            location_draft: None,
            place_draft: None,
            chooser: None,
            listing_worker: ListingWorker::new(),
            _keystroke_subscription: None,
            rubber_band: None,
            context_menu: None,
            properties_dialog: None,
            pane_statuses: HashMap::new(),
            operation_pending: false,
            operation_pane: None,
            operation_progress: None,
        }
    }

    fn test_entry(name: &str) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    fn test_entries(names: &[&str]) -> Arc<Vec<fika_core::Entry>> {
        Arc::new(names.iter().map(|name| test_entry(name)).collect())
    }

    fn status_entry(
        id: u64,
        name: &'static str,
        is_dir: bool,
        size_bytes: u64,
    ) -> fika_core::ModelEntry {
        fika_core::ModelEntry {
            id: fika_core::ItemId(id),
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from(name),
                name_width_units: name.len() as u16,
                size_bytes,
                modified_secs: None,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir,
            }),
        }
    }

    fn context_blank_target() -> ContextMenuTarget {
        ContextMenuTarget::Blank {
            trash_view: false,
            trash_has_items: false,
        }
    }

    fn context_item_target(path: &str, is_dir: bool, selection_count: usize) -> ContextMenuTarget {
        ContextMenuTarget::Item {
            path: PathBuf::from(path),
            is_dir,
            selection_count,
            trash_view: false,
            trash_can_restore: false,
        }
    }

    fn context_place_target(
        path: PathBuf,
        trash_place: bool,
        trash_has_items: bool,
    ) -> ContextMenuTarget {
        ContextMenuTarget::Place {
            path,
            trash_place,
            trash_has_items,
            editable: false,
            removable: false,
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("fika-gpui-{name}-{}-{nanos}", std::process::id()))
    }
}
