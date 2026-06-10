mod ui;

use fika_core::{
    CompactColumnMetrics, CompactLayout, CompactLayoutOptions, CreateUndoItem, CreatedItemKind,
    DirectoryCache, DirectoryLister, DirectoryListerEvent, OperationQueue, PaneController, PaneId,
    RenameUndoItem, SelectionMove, TransferUndoItem, TrashUndoItem, UndoPayload, UndoRecord,
    ViewPoint, ViewRect, ViewState, ZoomChange, file_ops, nearest_existing_ancestor,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, Div, IntoElement, MouseButton, ParentElement, Render,
    Stateful, Styled, Window, WindowBounds, WindowOptions, div, px, rgb, rgba, size,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, UNIX_EPOCH};

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
    item_count: usize,
    layout: CompactLayout,
    visible_items: Vec<VisibleItemSnapshot>,
    view: ViewState,
    rubber_band: Option<ViewRect>,
    focused: bool,
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
}

#[derive(Clone, Debug, PartialEq)]
enum ContextMenuTarget {
    Blank,
    Item {
        path: PathBuf,
        is_dir: bool,
        selection_count: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMenuAction {
    Open,
    OpenInNewPane,
    Rename,
    Copy,
    CopyLocation,
    Cut,
    Trash,
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
}

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
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlaceEntry {
    group: &'static str,
    marker: &'static str,
    label: String,
    path: PathBuf,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClipboardState {
    mode: ClipboardMode,
    paths: Vec<PathBuf>,
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
        let key = CompactColumnWidthCacheKey {
            generation: model.data_generation(),
            item_count: model.len(),
            rows_per_column,
            min_item_width: options.item_width,
            icon_size: options.icon_size,
            padding: options.padding,
            gap: options.gap,
        };
        let column_count = model.len().div_ceil(rows_per_column);
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
        entry.resolve_visible_columns(model, rows_per_column, options);
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
        rows_per_column: usize,
        options: CompactLayoutOptions,
    ) {
        if self.widths.is_empty() {
            return;
        }

        for _ in 0..2 {
            let metrics = self.metrics(options);
            let layout = CompactLayout::new_with_column_metrics(model.len(), options, metrics);
            let range = overscanned_column_range(
                layout.visible_column_range(),
                self.widths.len(),
                CompactColumnWidthCache::COLUMN_OVERSCAN,
            );
            if range.is_empty() || !self.resolve_columns(model, rows_per_column, options, range) {
                break;
            }
        }
    }

    fn resolve_columns(
        &mut self,
        model: &fika_core::DirectoryModel,
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
            let end = (start + rows_per_column).min(model.len());
            let mut width = options.item_width;
            for entry in &model.entries()[start..end] {
                width = width.max(required_compact_item_width(entry, options));
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

fn format_entry_kind_label(entry: &fika_core::EntryData) -> String {
    if let Some(label) = &entry.trash_deletion_label {
        return label.to_string();
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
    let options = ui::file_grid::compact_layout_options(view, 0.0);
    let rows_per_column = CompactLayout::rows_per_column_for_options(options);
    let metrics = cache.metrics_for_model(model, rows_per_column, options);
    let layout = CompactLayout::new_with_column_metrics(model.len(), options, metrics);

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
    let metrics = cache.metrics_for_model(model, rows_per_column, options);
    CompactLayout::new_with_column_metrics(model.len(), options, metrics)
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
    file_icons: FileIconCache,
    viewport_origins: HashMap<PaneId, ViewPoint>,
    visible_item_slots: HashMap<PaneId, VisibleItemSlotPool>,
    compact_column_widths: HashMap<PaneId, CompactColumnWidthCache>,
    operations: OperationQueue,
    clipboard: Option<ClipboardState>,
    rename_draft: Option<RenameDraft>,
    location_draft: Option<LocationDraft>,
    chooser: Option<ChooserState>,
    listing_worker: ListingWorker,
    _keystroke_subscription: Option<gpui::Subscription>,
    pub(crate) rubber_band: Option<RubberBandState>,
    context_menu: Option<ContextMenuState>,
    properties_dialog: Option<PropertiesDialogState>,
    operation_pending: bool,
    status: String,
}

impl FikaApp {
    fn new(args: Args, cx: &mut Context<Self>) -> Self {
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
            places: build_places(),
            file_icons: FileIconCache::default(),
            viewport_origins: HashMap::new(),
            visible_item_slots: HashMap::new(),
            compact_column_widths: HashMap::new(),
            operations: OperationQueue::new(),
            clipboard: None,
            rename_draft: None,
            location_draft: None,
            chooser,
            listing_worker: ListingWorker::new(),
            _keystroke_subscription: None,
            rubber_band: None,
            context_menu: None,
            properties_dialog: None,
            operation_pending: false,
            status: String::new(),
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
                    async_io::Timer::after(Duration::from_millis(350)).await;
                    if this
                        .update(&mut cx, |app, cx| {
                            if app.drain_background_listing_results() | app.drain_watchers() {
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

    fn snapshots(&mut self) -> Vec<PaneSnapshot> {
        let focused_pane = self.panes.focused();
        let pane_ids = self.panes.pane_ids().to_vec();
        pane_ids
            .into_iter()
            .filter_map(|pane_id| {
                let (
                    breadcrumbs,
                    location_draft,
                    item_count,
                    layout,
                    view,
                    rubber_band,
                    focused,
                    visible_data,
                ) = {
                    let pane = self.panes.pane(pane_id)?;
                    let rename_draft = self
                        .rename_draft
                        .as_ref()
                        .filter(|draft| draft.pane_id == pane_id);
                    let location_draft = self
                        .location_draft
                        .as_ref()
                        .filter(|draft| draft.pane_id == pane_id)
                        .map(|draft| draft.value.clone());
                    let item_count = pane.model.len();
                    let layout = compact_layout_for_model(
                        self.compact_column_widths.entry(pane_id).or_default(),
                        &pane.model,
                        &pane.view,
                    );
                    let visible_data = layout
                        .visible_items()
                        .filter_map(|visible_item| {
                            let entry = pane.model.get(visible_item.model_index)?;
                            let path = pane.model.path_for_index(visible_item.model_index)?;
                            let item_layout = layout.item_with_required_text_width(
                                visible_item.model_index,
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
                        item_count,
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
                Some(PaneSnapshot {
                    id: pane_id,
                    breadcrumbs,
                    location_draft,
                    item_count,
                    layout,
                    visible_items,
                    view,
                    rubber_band,
                    focused,
                })
            })
            .collect()
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
            .map(|(index, place)| PlaceSnapshot {
                group: place.group,
                marker: place.marker,
                label: place.label.clone(),
                path: place.path.clone(),
                active: active_index == Some(index),
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

    fn load_pane(&mut self, pane_id: PaneId, path: PathBuf) {
        let Some(event) = self.panes.load(pane_id, path.clone()) else {
            return;
        };
        self.clear_pane_transient_state(pane_id);
        let cached_events = self.schedule_listing(&event);
        self.apply_event(event);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.status = format!("Loading {}", path.display());
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
            self.status = format!("Reloading {}", path.display());
        }
    }

    fn go_back(&mut self, pane_id: PaneId) {
        let Some(event) = self.panes.go_back(pane_id) else {
            return;
        };
        self.clear_pane_transient_state(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.apply_event(event);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.status = format!("Loading {}", path.display());
    }

    fn go_forward(&mut self, pane_id: PaneId) {
        let Some(event) = self.panes.go_forward(pane_id) else {
            return;
        };
        self.clear_pane_transient_state(pane_id);
        let path = event.path().to_path_buf();
        let cached_events = self.schedule_listing(&event);
        self.apply_event(event);
        self.apply_cached_listing_events(cached_events);
        self.start_watcher(pane_id);
        self.status = format!("Loading {}", path.display());
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
        self.status = format!("Split pane {}", new_id.0);
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
            self.status = format!("Closed pane {}", pane_id.0);
        }
    }

    fn clear_pane_transient_state(&mut self, pane_id: PaneId) {
        self.visible_item_slots.remove(&pane_id);
        self.compact_column_widths.remove(&pane_id);
        self.viewport_origins.remove(&pane_id);
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
    }

    fn select_only(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.select_only(pane_id, path) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.status = format!("{selected} selected");
        }
    }

    fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) {
        if self.panes.toggle_selection(pane_id, path).is_some() {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            let selected = self.panes.selected_count(pane_id).unwrap_or_default();
            self.status = format!("{selected} selected");
        }
    }

    fn select_range_to(&mut self, pane_id: PaneId, path: PathBuf) {
        if let Some(selected) = self.panes.select_range_to(pane_id, path) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.status = format!("{selected} selected");
        }
    }

    fn select_all(&mut self, pane_id: PaneId) {
        if let Some(selected) = self.panes.select_all(pane_id) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.status = format!("{selected} selected");
        }
    }

    fn clear_selection(&mut self, pane_id: PaneId) {
        if self.panes.clear_selection(pane_id) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.status = "Selection cleared".to_string();
        }
    }

    fn move_selection(&mut self, pane_id: PaneId, direction: SelectionMove, extend: bool) {
        if let Some(selected) = self.panes.move_selection(pane_id, direction, extend) {
            self.clear_rename_draft_for_pane(pane_id);
            self.clear_location_draft_for_pane(pane_id);
            self.status = format!("{selected} selected");
        }
    }

    fn apply_zoom_change(&mut self, pane_id: PaneId, change: ZoomChange) {
        let Some(previous_level) = self.panes.pane(pane_id).map(|pane| pane.view.zoom_level)
        else {
            return;
        };
        let Some(view) = self.panes.apply_zoom_change(pane_id, change) else {
            return;
        };
        if view.zoom_level == previous_level {
            self.status = format!(
                "Zoom level {} ({} px)",
                view.zoom_level,
                view.icon_size() as i32
            );
            return;
        }
        self.compact_column_widths.remove(&pane_id);
        self.status = format!(
            "Zoom level {} ({} px)",
            view.zoom_level,
            view.icon_size() as i32
        );
    }

    fn set_viewport_origin(&mut self, pane_id: PaneId, origin: ViewPoint) -> bool {
        if self.viewport_origins.get(&pane_id) == Some(&origin) {
            return false;
        }
        self.viewport_origins.insert(pane_id, origin);
        true
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

    fn layout_for_pane(&mut self, pane_id: PaneId) -> Option<CompactLayout> {
        let pane = self.panes.pane(pane_id)?;
        Some(compact_layout_for_model(
            self.compact_column_widths.entry(pane_id).or_default(),
            &pane.model,
            &pane.view,
        ))
    }

    fn item_at_content_point(
        &mut self,
        pane_id: PaneId,
        point: ViewPoint,
    ) -> Option<ContentItemHit> {
        let layout = self.layout_for_pane(pane_id)?;
        let model_index = layout.hit_test_content_point(point)?;
        let pane = self.panes.pane(pane_id)?;
        let entry = pane.model.get(model_index)?;
        let item_layout = layout.item_with_required_text_width(
            model_index,
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
        let Some(layout) = self.layout_for_pane(pane_id) else {
            return Vec::new();
        };
        let candidate_indexes = layout.indexes_intersecting(rect).indexes().to_vec();
        let Some(pane) = self.panes.pane(pane_id) else {
            return Vec::new();
        };
        candidate_indexes
            .into_iter()
            .filter(|index| {
                let Some(entry) = pane.model.get(*index) else {
                    return false;
                };
                layout
                    .item_with_required_text_width(
                        *index,
                        Some(compact_text_width(entry.name_width_units)),
                    )
                    .is_some_and(|item| item.visual_rect.intersects(rect))
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
            self.status = format!("{selected} selected");
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
        self.location_draft = Some(LocationDraft {
            pane_id,
            value: path.display().to_string(),
        });
        self.status = format!("Location {}", path.display());
    }

    pub(crate) fn open_location_segment(&mut self, pane_id: PaneId, path: PathBuf) {
        self.panes.focus(pane_id);
        self.clear_location_draft_for_pane(pane_id);
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
                self.status = "Location edit cancelled".to_string();
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
            self.status = "Location is empty".to_string();
            return;
        };
        if !path.is_dir() {
            self.status = format!("Location is not a folder: {}", path.display());
            return;
        }
        if path == current_dir {
            self.status = format!("Location {}", path.display());
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
            self.status = "No location completion".to_string();
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
            self.status = "File operation already running".to_string();
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        let [original_path] = selected_paths.as_slice() else {
            self.status = "Select one item to rename".to_string();
            return;
        };
        let Some(name) = original_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
        else {
            self.status = "Selected item cannot be renamed".to_string();
            return;
        };

        self.clear_location_draft_for_pane(pane_id);
        self.rename_draft = Some(RenameDraft {
            pane_id,
            original_path: original_path.clone(),
            draft_name: name.to_string(),
        });
        self.status = format!("Renaming {name}");
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
                self.status = "Rename cancelled".to_string();
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
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(draft) = self.rename_draft.take() else {
            return;
        };
        let new_name = draft.draft_name.trim().to_string();
        if new_name.is_empty() {
            self.status = "Name cannot be empty".to_string();
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
            self.status = "Rename unchanged".to_string();
            return;
        }

        self.operation_pending = true;
        self.status = format!("Renaming {}", draft.original_path.display());
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
        self.operation_pending = false;
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
                self.status = format!("Renamed to {}", renamed_path.display());
            }
            Err(err) => {
                self.status = format!("Cannot rename {}: {err}", result.original_path.display());
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
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(parent_dir) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };

        self.operation_pending = true;
        self.status = format!("Creating {}", created_item_label(kind).to_ascii_lowercase());
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
        self.operation_pending = false;
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
                self.status = format!("Created {}", path.display());
            }
            Err(err) => {
                self.status = format!(
                    "Cannot create {}: {err}",
                    created_item_label(result.kind).to_ascii_lowercase()
                );
            }
        }
    }

    fn store_selection_for_transfer(&mut self, pane_id: PaneId, mode: ClipboardMode) {
        if self.chooser.is_some() {
            return;
        }
        let paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if paths.is_empty() {
            self.status = format!("No selection to {}", mode.label().to_ascii_lowercase());
            return;
        }

        let count = paths.len();
        self.clipboard = Some(ClipboardState { mode, paths });
        self.status = format!("{} {} item(s)", mode.label(), count);
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
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(clipboard) = self.clipboard.clone() else {
            self.status = "Nothing to paste".to_string();
            return;
        };
        if !target_dir.is_dir() {
            self.status = format!("Cannot paste into {}", target_dir.display());
            return;
        }

        self.operation_pending = true;
        self.status = format!(
            "{}ing {} item(s)",
            clipboard.mode.label(),
            clipboard.paths.len()
        );
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move {
                            paste_clipboard_result(pane_id, target_dir, clipboard)
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
        self.operation_pending = false;
        if result.success_count > 0 {
            self.operations.register_undo_with_payload(
                result.mode.label().to_string(),
                result.affected_dirs.clone(),
                UndoPayload::Transfer {
                    items: result.undo_items,
                },
            );
            self.refresh_affected_dirs(&result.affected_dirs);
            if result.mode == ClipboardMode::Cut {
                self.clipboard = None;
                let _ = self.panes.clear_selection(result.pane_id);
            }
        }

        self.status = action_status(
            &format!("{} complete", result.mode.label()),
            result.success_count,
            result.failure_count,
        );
    }

    fn trash_selection(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let selected_paths = self.panes.selected_paths(pane_id).unwrap_or_default();
        if selected_paths.is_empty() {
            self.status = "No selection to trash".to_string();
            return;
        }

        self.operation_pending = true;
        self.status = format!("Moving {} item(s) to trash", selected_paths.len());
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
        self.operation_pending = false;
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

        self.status = action_status("Moved to trash", result.success_count, result.failure_count);
    }

    fn undo_latest(&mut self, cx: &mut Context<Self>) {
        if self.chooser.is_some() {
            return;
        }
        if self.operation_pending {
            self.status = "File operation already running".to_string();
            return;
        }
        let Some(record) = self.operations.latest_undo().cloned() else {
            self.status = "No operation to undo".to_string();
            return;
        };

        match &record.payload {
            UndoPayload::Create { .. } => {}
            UndoPayload::Rename { .. } => {}
            UndoPayload::Trash { .. } => {}
            UndoPayload::Transfer { .. } => {}
            UndoPayload::None => {
                self.status = format!("No undo action for {}", record.label);
                return;
            }
        }

        self.operation_pending = true;
        self.status = format!("Undoing {}", record.label);
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn(async move { undo_record_result(record) })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.finish_undo(result);
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn finish_undo(&mut self, result: UndoTaskResult) {
        self.operation_pending = false;
        match result.result {
            Ok(message) => {
                if self
                    .operations
                    .take_latest_undo(result.record.serial)
                    .is_none()
                {
                    self.status = "Undo result is stale".to_string();
                    return;
                }
                self.refresh_affected_dirs(&result.record.affected_dirs);
                self.status = format!("Undid {}: {message}", result.record.label);
            }
            Err(err) => {
                self.status = format!("Cannot undo {}: {err}", result.record.label);
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
        self.context_menu = Some(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Blank,
            position,
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
            },
            position: menu_position,
        });
    }

    fn dismiss_context_menu(&mut self) {
        self.context_menu = None;
    }

    fn dismiss_properties_dialog(&mut self) {
        self.properties_dialog = None;
    }

    fn show_properties_for_context(&mut self, pane_id: PaneId, target: ContextMenuTarget) {
        let dialog = match target {
            ContextMenuTarget::Blank => {
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
                self.status = format!("Open with is not implemented for {}", path.display());
            }
            (ContextMenuAction::Rename, ContextMenuTarget::Item { path, .. }) => {
                self.select_only(menu.pane_id, path);
                self.start_rename_in_pane(menu.pane_id);
            }
            (ContextMenuAction::Copy, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Copy, ContextMenuTarget::Blank) => {
                self.store_selection_for_transfer(menu.pane_id, ClipboardMode::Copy)
            }
            (ContextMenuAction::CopyLocation, ContextMenuTarget::Item { path, .. }) => {
                let location = path.display().to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(location));
                self.status = format!("Copied location {}", path.display());
            }
            (ContextMenuAction::Cut, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Cut, ContextMenuTarget::Blank) => {
                self.store_selection_for_transfer(menu.pane_id, ClipboardMode::Cut)
            }
            (ContextMenuAction::Trash, ContextMenuTarget::Item { .. })
            | (ContextMenuAction::Trash, ContextMenuTarget::Blank) => {
                self.trash_selection(menu.pane_id, cx)
            }
            (ContextMenuAction::Properties, target) => {
                self.show_properties_for_context(menu.pane_id, target)
            }
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
            (ContextMenuAction::Open, ContextMenuTarget::Blank)
            | (ContextMenuAction::CopyLocation, ContextMenuTarget::Blank)
            | (ContextMenuAction::OpenInNewPane, _)
            | (ContextMenuAction::Rename, ContextMenuTarget::Blank) => {}
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
        let Some(pane_id) = self.panes.focused() else {
            return false;
        };
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
            Some(PaneShortcut::MoveSelection { direction, extend }) => {
                self.move_selection(pane_id, direction, extend)
            }
            Some(PaneShortcut::CreateFolder) => {
                self.create_item_in_pane(pane_id, CreatedItemKind::Folder, cx)
            }
            Some(PaneShortcut::RenameSelection) => self.start_rename_in_pane(pane_id),
            Some(PaneShortcut::CopySelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Copy)
            }
            Some(PaneShortcut::CutSelection) => {
                self.store_selection_for_transfer(pane_id, ClipboardMode::Cut)
            }
            Some(PaneShortcut::PasteIntoPane) => self.paste_into_pane(pane_id, cx),
            Some(PaneShortcut::TrashSelection) => self.trash_selection(pane_id, cx),
            Some(PaneShortcut::Undo) => self.undo_latest(cx),
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
            self.status = "No chooser selection".to_string();
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
        if let DirectoryListerEvent::CurrentDirectoryRemoved { pane_id, path, .. } = &event {
            self.listing_worker.remove_cached_directory(path);
            let still_current = self.panes.pane(*pane_id).is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            });
            if still_current {
                let fallback =
                    nearest_existing_ancestor(path).unwrap_or_else(|| PathBuf::from("/"));
                self.status = format!("{} was removed", path.display());
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

        if let Some(signals) = self.panes.apply_lister_event(event) {
            if !signals.is_empty() {
                self.status = format!("{} model signal(s)", signals.len());
            }
        }
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
        if let Err(err) = pane.lister.start_watcher() {
            self.status = format!("Cannot watch {}: {err}", pane.current_dir.display());
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
        let snapshots = self.snapshots();
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
        let clipboard_available = self.clipboard.is_some();
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
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(rgb(0xc8ced6))
                    .bg(rgb(0xffffff))
                    .text_xs()
                    .text_color(rgb(0x59636e))
                    .child(if self.status.is_empty() {
                        "Ready".to_string()
                    } else {
                        self.status.clone()
                    }),
            )
            .when_some(context_menu, |root, menu| {
                root.child(context_menu_overlay(menu, clipboard_available, cx))
            })
            .when_some(properties_dialog, |root, dialog| {
                root.child(properties_dialog_overlay(dialog, cx))
            })
    }
}

fn context_menu_overlay(
    menu: ContextMenuState,
    clipboard_available: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let actions = context_menu_actions(&menu.target, clipboard_available);
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
                .w(px(196.0))
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
                        .map(|action| context_menu_row(action, cx)),
                ),
        )
}

fn context_menu_actions(
    target: &ContextMenuTarget,
    clipboard_available: bool,
) -> Vec<ContextMenuItem> {
    match target {
        ContextMenuTarget::Blank => vec![
            context_menu_item(ContextMenuAction::CreateFolder, "New Folder"),
            ContextMenuItem {
                action: ContextMenuAction::Paste,
                label: "Paste",
                enabled: clipboard_available,
            },
            context_menu_item(ContextMenuAction::SelectAll, "Select All"),
            context_menu_item(ContextMenuAction::Refresh, "Refresh"),
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
    }
}

fn context_menu_row(item: ContextMenuItem, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let action = item.action;
    div()
        .id(format!("context-menu-action-{action:?}"))
        .px_3()
        .py_1()
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
                    this.run_context_menu_action(action, cx);
                    cx.stop_propagation();
                    cx.notify();
                }))
        })
        .child(item.label)
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
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "n" => Some(PaneShortcut::CreateFolder),
            _ => None,
        };
    }

    if keystroke.modifiers.secondary() && keystroke.modifiers.number_of_modifiers() == 1 {
        return match keystroke.key.to_ascii_lowercase().as_str() {
            "a" => Some(PaneShortcut::SelectAll),
            "c" => Some(PaneShortcut::CopySelection),
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

#[derive(Clone, Debug)]
struct PasteTaskResult {
    pane_id: PaneId,
    mode: ClipboardMode,
    success_count: usize,
    failure_count: usize,
    affected_dirs: Vec<PathBuf>,
    undo_items: Vec<TransferUndoItem>,
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
) -> PasteTaskResult {
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut undo_items = Vec::new();
    let operation = clipboard.mode.operation();

    for source in clipboard.paths {
        match file_ops::perform_transfer_with_progress_outcome(
            operation,
            &source,
            &target_dir,
            "keep-both",
            None,
            |_| {},
        ) {
            Ok(outcome) => {
                success_count += 1;
                push_unique_path(&mut affected_dirs, target_dir.clone());
                if clipboard.mode == ClipboardMode::Cut
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
        mode: clipboard.mode,
        success_count,
        failure_count,
        affected_dirs,
        undo_items,
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

fn build_places() -> Vec<PlaceEntry> {
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
    });
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
            },
            PlaceEntry {
                group: "",
                marker: "H",
                label: "Home".to_string(),
                path: PathBuf::from("/home/yk"),
            },
            PlaceEntry {
                group: "",
                marker: "Down",
                label: "Downloads".to_string(),
                path: PathBuf::from("/home/yk/Downloads"),
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
        let blank = ContextMenuTarget::Blank;
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
    fn context_menu_actions_offer_new_pane_only_for_directories() {
        let dir_target = ContextMenuTarget::Item {
            path: PathBuf::from("/tmp"),
            is_dir: true,
            selection_count: 1,
        };
        let file_target = ContextMenuTarget::Item {
            path: PathBuf::from("/tmp/readme.txt"),
            is_dir: false,
            selection_count: 1,
        };

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
        let dir_target = ContextMenuTarget::Item {
            path: PathBuf::from("/tmp"),
            is_dir: true,
            selection_count: 1,
        };
        let file_target = ContextMenuTarget::Item {
            path: PathBuf::from("/tmp/readme.txt"),
            is_dir: false,
            selection_count: 1,
        };

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
        let target = ContextMenuTarget::Item {
            path: PathBuf::from("/tmp/readme.txt"),
            is_dir: false,
            selection_count: 3,
        };
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
            ClipboardState {
                mode: ClipboardMode::Copy,
                paths: vec![source.clone()],
            },
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
            ClipboardState {
                mode: ClipboardMode::Cut,
                paths: vec![source.clone()],
            },
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
            ClipboardState {
                mode: ClipboardMode::Cut,
                paths: vec![source.clone()],
            },
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
                    trash_group: None,
                    trash_deletion_label: None,
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
            trash_group: None,
            trash_deletion_label: None,
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

    fn test_entries(names: &[&str]) -> Arc<Vec<fika_core::Entry>> {
        Arc::new(
            names
                .iter()
                .map(|name| {
                    fika_core::Entry::new(fika_core::EntryData {
                        name: Arc::from(*name),
                        name_width_units: name.len() as u16,
                        size_bytes: 0,
                        modified_secs: None,
                        trash_group: None,
                        trash_deletion_label: None,
                        is_dir: false,
                    })
                })
                .collect(),
        )
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("fika-gpui-{name}-{}-{nanos}", std::process::id()))
    }
}
