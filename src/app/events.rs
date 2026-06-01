use crate::DeviceEntry;
use crate::desktop::open_with;
use crate::fs::entries::RawFileEntry;
use crate::fs::{file_actions, file_ops, privilege, search, thumbnails};
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct DirectoryLoadResult {
    pub(crate) generation: u64,
    pub(crate) path: PathBuf,
    pub(crate) preserve_view: bool,
    pub(crate) result: io::Result<Vec<RawFileEntry>>,
}

#[derive(Debug)]
pub(crate) struct FileOpenResult {
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
    pub(crate) operation: String,
    pub(crate) session: privilege::ExternalEditSession,
    pub(crate) result: Result<PathBuf, String>,
}

#[derive(Debug)]
pub(crate) struct RecursiveSearchResult {
    pub(crate) generation: u64,
    pub(crate) query: String,
    pub(crate) root: PathBuf,
    pub(crate) result: io::Result<Vec<RawFileEntry>>,
}

#[derive(Debug)]
pub(crate) struct RecursiveSearchProgress {
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
pub(crate) enum AsyncEvent {
    DirectoryLoaded(DirectoryLoadResult),
    FileOpened(FileOpenResult),
    RecursiveSearchProgress(RecursiveSearchProgress),
    RecursiveSearchFinished(RecursiveSearchResult),
    OpenWithAppsLoaded(open_with::OpenWithAppsResult),
    OtherApplicationAppsLoaded(open_with::OtherApplicationAppsResult),
    DefaultAppSet(open_with::DefaultAppSetResult),
    FileActionFinished(file_actions::FileActionResult),
    FileOperationProgress(FileOperationProgress),
    FileOperationFinished(FileOperationResult),
    FileUndoFinished(FileUndoResult),
    DeviceMountFinished(DeviceMountResult),
    DeviceActionFinished(DeviceActionResult),
    DevicesChanged,
    DevicesLoaded(DevicesLoadedResult),
    PrivilegedOperationFinished(privilege::PrivilegedOperationResult),
    ExternalEditFinished(ExternalEditResult),
    ThumbnailLoaded {
        generation: u64,
        load: thumbnails::ThumbnailLoad,
    },
    ExternalFileDropped(ExternalFileDrop),
}

#[derive(Debug)]
pub(crate) struct ExternalFileDrop {
    pub(crate) path: PathBuf,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) source: String,
}
