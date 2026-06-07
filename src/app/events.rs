use crate::DeviceEntry;
use crate::app::geometry::ItemViewLayoutEngine;
use crate::app::item_view_renderer::ItemViewRenderMetrics;
use crate::app::pane::PreparedDirectoryEntries;
use crate::app::virtual_view::VirtualViewSnapshotUpdate;
use crate::desktop::{clipboard, open_with, service_menu, systemd_launch};
use crate::fs::{file_actions, file_ops, privilege, search, thumbnails};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) const EXTERNAL_EDIT_SAVE_OPERATION: &str = "Admin Save";
pub(crate) const EXTERNAL_EDIT_DISCARD_OPERATION: &str = "Discard";

#[derive(Debug)]
pub(crate) struct DirectoryLoadResult {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) path: PathBuf,
    pub(crate) preserve_view: bool,
    pub(crate) defer_view_restore: bool,
    pub(crate) result: io::Result<PreparedDirectoryEntries>,
}

#[derive(Debug)]
pub(crate) struct FileOpenResult {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) path: PathBuf,
    pub(crate) result: Result<FileOpenSuccess, String>,
}

#[derive(Debug)]
pub(crate) struct FileOpenSuccess {
    pub(crate) mime_type: String,
    pub(crate) unit: Option<String>,
    pub(crate) launch_diagnostic: Option<String>,
    pub(crate) external_edit: Option<privilege::ExternalEditSession>,
}

#[derive(Debug)]
pub(crate) struct ExternalEditResult {
    pub(crate) pane_id: u64,
    pub(crate) operation: String,
    pub(crate) session: privilege::ExternalEditSession,
    pub(crate) result: Result<PathBuf, String>,
}

#[derive(Debug)]
pub(crate) struct RecursiveSearchResult {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) query: String,
    pub(crate) root: PathBuf,
    pub(crate) result: io::Result<PreparedDirectoryEntries>,
}

#[derive(Debug)]
pub(crate) struct RecursiveSearchProgress {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) query: String,
    pub(crate) root: PathBuf,
    pub(crate) progress: search::SearchProgress,
}

#[derive(Debug)]
pub(crate) struct FileOperationResult {
    pub(crate) id: u64,
    pub(crate) operation: String,
    pub(crate) source: PathBuf,
    pub(crate) target_dir: PathBuf,
    pub(crate) privileged_command: Option<privilege::PrivilegedCommand>,
    pub(crate) result: Result<file_ops::TransferOutcome, String>,
}

#[derive(Debug)]
pub(crate) struct FileOperationProgress {
    pub(crate) id: u64,
    pub(crate) operation: String,
    pub(crate) source: PathBuf,
    pub(crate) bytes_done: u64,
    pub(crate) bytes_total: u64,
}

#[derive(Debug)]
pub(crate) struct FileUndoResult {
    pub(crate) undo: crate::app::state::FileUndo,
    pub(crate) result: Result<String, String>,
}

#[derive(Debug)]
pub(crate) struct DeviceMountResult {
    pub(crate) device_path: String,
    pub(crate) result: Result<PathBuf, String>,
}

#[derive(Debug)]
pub(crate) struct DeviceActionResult {
    pub(crate) action: String,
    pub(crate) device_path: String,
    pub(crate) mount_path: Option<PathBuf>,
    pub(crate) result: Result<(), String>,
}

#[derive(Debug)]
pub(crate) struct DevicesLoadedResult {
    pub(crate) generation: u64,
    pub(crate) devices: Vec<DeviceEntry>,
}

#[derive(Debug)]
pub(crate) struct ClipboardLoadResult {
    pub(crate) generation: u64,
    pub(crate) result: Result<clipboard::ClipboardSnapshot, String>,
}

#[derive(Debug)]
pub(crate) struct ClipboardPasteLoadResult {
    pub(crate) generation: u64,
    pub(crate) target_dir: PathBuf,
    pub(crate) result: Result<clipboard::ClipboardSnapshot, String>,
}

#[derive(Debug)]
pub(crate) struct VirtualViewResult {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) thumbnail_size_px: u32,
    pub(crate) schedule_thumbnails: bool,
    pub(crate) cell_width: f32,
    pub(crate) render_metrics: ItemViewRenderMetrics,
    pub(crate) update: VirtualViewSnapshotUpdate,
}

#[derive(Debug)]
pub(crate) struct VirtualViewLayoutPrewarmResult {
    pub(crate) pane_id: u64,
    pub(crate) generation: u64,
    pub(crate) layouts: Vec<Arc<ItemViewLayoutEngine>>,
    pub(crate) finished: bool,
}

#[derive(Debug)]
pub(crate) struct ServiceMenuActionLaunchResult {
    pub(crate) pane_id: u64,
    pub(crate) action_name: String,
    pub(crate) result: Result<systemd_launch::LaunchResult, String>,
}

#[derive(Debug)]
pub(crate) enum AsyncEvent {
    DirectoryLoaded(DirectoryLoadResult),
    DirectoryPrefetched {
        path: PathBuf,
        result: io::Result<PreparedDirectoryEntries>,
    },
    FileOpened(FileOpenResult),
    RecursiveSearchProgress(RecursiveSearchProgress),
    RecursiveSearchFinished(RecursiveSearchResult),
    OpenWithAppsLoaded(open_with::OpenWithAppsResult),
    OtherApplicationAppsLoaded(open_with::OtherApplicationAppsResult),
    DefaultAppSet(open_with::DefaultAppSetResult),
    ServiceMenuActionsLoaded(service_menu::ServiceMenuActionsResult),
    ServiceMenuActionFinished(ServiceMenuActionLaunchResult),
    FileActionFinished(file_actions::FileActionResult),
    FileOperationProgress(FileOperationProgress),
    FileOperationFinished(FileOperationResult),
    FileUndoFinished(FileUndoResult),
    DeviceMountFinished(DeviceMountResult),
    DeviceActionFinished(DeviceActionResult),
    DevicesChanged,
    DevicesLoaded(DevicesLoadedResult),
    ClipboardLoaded(ClipboardLoadResult),
    ClipboardPasteLoaded(ClipboardPasteLoadResult),
    VirtualViewPrepared(VirtualViewResult),
    VirtualViewLayoutsPrewarmed(VirtualViewLayoutPrewarmResult),
    VirtualViewPrepareFailed {
        pane_id: u64,
        generation: u64,
    },
    PrivilegedOperationFinished(privilege::PrivilegedOperationResult),
    ExternalEditFinished(ExternalEditResult),
    ThumbnailLoaded {
        pane_id: u64,
        generation: u64,
        load: thumbnails::ThumbnailLoad,
    },
}
