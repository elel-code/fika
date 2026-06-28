use crate::core::operation_runtime::OperationController;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use compio::buf::{BufResult, IntoInner};
use compio::io::{AsyncReadAt, AsyncWriteAtExt};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Clone, Debug)]
pub struct TransferOutcome {
    pub destination: PathBuf,
    pub overwritten_backup: Option<PathBuf>,
}

struct TransferPlan {
    destination: PathBuf,
    overwritten_backup: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashMetadata {
    pub original_path: PathBuf,
    pub deletion_date: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferTargetRelation {
    Same,
    Descendant,
}

pub fn perform_transfer_with_progress(
    operation: &str,
    source: &Path,
    target_dir: &Path,
    conflict_policy: &str,
    controller: Option<OperationController>,
    progress: impl FnMut(TransferProgress),
) -> Result<PathBuf, String> {
    let outcome = perform_transfer_with_progress_outcome(
        operation,
        source,
        target_dir,
        conflict_policy,
        controller,
        progress,
    )?;
    if let Some(backup) = outcome.overwritten_backup {
        remove_path(&backup).map_err(|err| err.to_string())?;
    }
    Ok(outcome.destination)
}

pub fn perform_transfer_with_progress_outcome(
    operation: &str,
    source: &Path,
    target_dir: &Path,
    conflict_policy: &str,
    controller: Option<OperationController>,
    mut progress: impl FnMut(TransferProgress),
) -> Result<TransferOutcome, String> {
    if !path_exists(source) {
        return Err("source no longer exists".to_string());
    }
    if !target_dir.is_dir() {
        return Err("target is not a folder".to_string());
    }
    if let Some(relation) = transfer_target_relation(source, target_dir) {
        return Err(transfer_target_relation_error(relation).to_string());
    }

    let file_name = source
        .file_name()
        .ok_or_else(|| "source has no file name".to_string())?;
    let destination = transfer_destination(source, target_dir, file_name, conflict_policy)?;

    if !matches!(operation, "move" | "copy" | "link") {
        return Err(format!("unknown operation: {operation}"));
    }

    let overwrite_backup = if conflict_policy == "overwrite" && path_exists(&destination) {
        Some(backup_existing_destination(&destination)?)
    } else {
        None
    };

    let result = match operation {
        "move" => move_path(source, &destination, controller.as_ref(), &mut progress),
        "copy" => copy_path(source, &destination, controller.as_ref(), &mut progress),
        "link" => link_path(source, &destination),
        _ => unreachable!("operation was validated before dispatch"),
    };

    match result {
        Ok(()) => {
            // Keep the backup in the richer outcome so callers that expose Undo can restore it.
            // The simpler transfer API removes it immediately for one-shot operations.
        }
        Err(err) => {
            if let Some(backup) = overwrite_backup {
                restore_overwrite_backup(&destination, &backup)?;
            } else if err.kind() == io::ErrorKind::Interrupted {
                let _ = remove_path(&destination);
            }
            return Err(err.to_string());
        }
    }

    Ok(TransferOutcome {
        destination,
        overwritten_backup: overwrite_backup,
    })
}

pub async fn perform_transfer_with_progress_outcome_async(
    operation: &str,
    source: &Path,
    target_dir: &Path,
    conflict_policy: &str,
    controller: Option<OperationController>,
    mut progress: impl FnMut(TransferProgress),
) -> Result<TransferOutcome, String> {
    let plan = prepare_transfer_async(
        operation.to_string(),
        source.to_path_buf(),
        target_dir.to_path_buf(),
        conflict_policy.to_string(),
    )
    .await?;

    let result = match operation {
        "move" => {
            move_path_async(
                source,
                &plan.destination,
                controller.as_ref(),
                &mut progress,
            )
            .await
        }
        "copy" => {
            copy_path_async(
                source,
                &plan.destination,
                controller.as_ref(),
                &mut progress,
            )
            .await
        }
        "link" => link_path_async(source, &plan.destination).await,
        _ => unreachable!("operation was validated before dispatch"),
    };

    match result {
        Ok(()) => {}
        Err(err) => {
            if let Some(backup) = plan.overwritten_backup.as_ref() {
                restore_overwrite_backup_async(&plan.destination, backup).await?;
            } else if err.kind() == io::ErrorKind::Interrupted {
                let _ = remove_path_async(&plan.destination).await;
            }
            return Err(err.to_string());
        }
    }

    Ok(TransferOutcome {
        destination: plan.destination,
        overwritten_backup: plan.overwritten_backup,
    })
}

async fn prepare_transfer_async(
    operation: String,
    source: PathBuf,
    target_dir: PathBuf,
    conflict_policy: String,
) -> Result<TransferPlan, String> {
    if !path_exists_async(&source).await {
        return Err("source no longer exists".to_string());
    }
    if !metadata_async(&target_dir)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("target is not a folder".to_string());
    }
    if let Some(relation) = transfer_target_relation_async(&source, &target_dir).await {
        return Err(transfer_target_relation_error(relation).to_string());
    }

    let file_name = source
        .file_name()
        .ok_or_else(|| "source has no file name".to_string())?
        .to_os_string();
    let destination =
        transfer_destination_async(&source, &target_dir, &file_name, &conflict_policy).await?;

    if !matches!(operation.as_str(), "move" | "copy" | "link") {
        return Err(format!("unknown operation: {operation}"));
    }

    let overwritten_backup =
        if conflict_policy == "overwrite" && path_exists_async(&destination).await {
            Some(backup_existing_destination_async(&destination).await?)
        } else {
            None
        };

    Ok(TransferPlan {
        destination,
        overwritten_backup,
    })
}

pub fn base_destination(source: &Path, target_dir: &Path) -> Result<PathBuf, String> {
    let file_name = source
        .file_name()
        .ok_or_else(|| "source has no file name".to_string())?;
    Ok(target_dir.join(file_name))
}

pub fn transfer_target_relation(
    source: &Path,
    target_dir: &Path,
) -> Option<TransferTargetRelation> {
    if source == target_dir || canonical_paths_equal(source, target_dir) {
        return Some(TransferTargetRelation::Same);
    }
    if target_is_source_descendant(source, target_dir) {
        return Some(TransferTargetRelation::Descendant);
    }
    None
}

async fn transfer_target_relation_async(
    source: &Path,
    target_dir: &Path,
) -> Option<TransferTargetRelation> {
    if source == target_dir || canonical_paths_equal_async(source, target_dir).await {
        return Some(TransferTargetRelation::Same);
    }
    if target_is_source_descendant_async(source, target_dir).await {
        return Some(TransferTargetRelation::Descendant);
    }
    None
}

pub fn target_is_source_or_descendant(source: &Path, target_dir: &Path) -> bool {
    transfer_target_relation(source, target_dir).is_some()
}

fn transfer_target_relation_error(relation: TransferTargetRelation) -> &'static str {
    match relation {
        TransferTargetRelation::Same => "cannot transfer an item onto itself",
        TransferTargetRelation::Descendant => "cannot transfer a folder into itself",
    }
}

fn canonical_paths_equal(source: &Path, target_dir: &Path) -> bool {
    let Ok(source) = source.canonicalize() else {
        return false;
    };
    let Ok(target_dir) = target_dir.canonicalize() else {
        return false;
    };
    source == target_dir
}

async fn canonical_paths_equal_async(source: &Path, target_dir: &Path) -> bool {
    let Ok(source) = canonicalize_async(source).await else {
        return false;
    };
    let Ok(target_dir) = canonicalize_async(target_dir).await else {
        return false;
    };
    source == target_dir
}

fn target_is_source_descendant(source: &Path, target_dir: &Path) -> bool {
    if target_dir.starts_with(source) {
        return true;
    }

    let Ok(source) = source.canonicalize() else {
        return false;
    };
    let Ok(target_dir) = target_dir.canonicalize() else {
        return false;
    };
    target_dir.starts_with(source)
}

async fn target_is_source_descendant_async(source: &Path, target_dir: &Path) -> bool {
    if target_dir.starts_with(source) {
        return true;
    }

    let Ok(source) = canonicalize_async(source).await else {
        return false;
    };
    let Ok(target_dir) = canonicalize_async(target_dir).await else {
        return false;
    };
    target_dir.starts_with(source)
}

pub fn renamed_destination(target_dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !target_dir.is_dir() {
        return Err("target is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = target_dir.join(name);
    if path_exists(&destination) {
        return Err("an item with that name already exists".to_string());
    }
    Ok(destination)
}

pub fn undo_transfer(
    operation: &str,
    original_source: &Path,
    destination: &Path,
) -> Result<String, String> {
    undo_transfer_with_backup(operation, original_source, destination, None)
}

pub fn undo_transfer_with_backup(
    operation: &str,
    original_source: &Path,
    destination: &Path,
    overwritten_backup: Option<&Path>,
) -> Result<String, String> {
    if let Some(backup) = overwritten_backup {
        return undo_overwrite_transfer(operation, original_source, destination, backup);
    }

    match operation {
        "copy" | "link" => {
            if !path_exists(destination) {
                return Err("undo target no longer exists".to_string());
            }
            remove_path(destination).map_err(|err| err.to_string())?;
            Ok(format!("removed {}", destination.display()))
        }
        "move" => {
            if !path_exists(destination) {
                return Err("moved item no longer exists".to_string());
            }
            if path_exists(original_source) {
                return Err("original location is already occupied".to_string());
            }
            if let Some(parent) = original_source.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            match fs::rename(destination, original_source) {
                Ok(()) => Ok(format!("restored {}", original_source.display())),
                Err(err) if err.raw_os_error() == Some(18) => {
                    copy_path(destination, original_source, None, &mut |_| {})
                        .map_err(|err| err.to_string())?;
                    remove_path(destination).map_err(|err| err.to_string())?;
                    Ok(format!("restored {}", original_source.display()))
                }
                Err(err) => Err(err.to_string()),
            }
        }
        _ => Err(format!("cannot undo operation: {operation}")),
    }
}

fn undo_overwrite_transfer(
    operation: &str,
    original_source: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<String, String> {
    if !path_exists(backup) {
        return Err("overwritten item backup no longer exists".to_string());
    }

    match operation {
        "copy" | "link" => {
            if path_exists(destination) {
                remove_path(destination).map_err(|err| err.to_string())?;
            }
            fs::rename(backup, destination).map_err(|err| err.to_string())?;
            Ok(format!("restored overwritten {}", destination.display()))
        }
        "move" => {
            if !path_exists(destination) {
                return Err("moved item no longer exists".to_string());
            }
            if path_exists(original_source) {
                return Err("original location is already occupied".to_string());
            }
            if let Some(parent) = original_source.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            match fs::rename(destination, original_source) {
                Ok(()) => {}
                Err(err) if err.raw_os_error() == Some(18) => {
                    copy_path(destination, original_source, None, &mut |_| {})
                        .map_err(|err| err.to_string())?;
                    remove_path(destination).map_err(|err| err.to_string())?;
                }
                Err(err) => return Err(err.to_string()),
            }
            fs::rename(backup, destination).map_err(|err| err.to_string())?;
            Ok(format!(
                "restored {} and overwritten {}",
                original_source.display(),
                destination.display()
            ))
        }
        _ => Err(format!("cannot undo operation: {operation}")),
    }
}

pub fn create_folder(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !parent.is_dir() {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination(parent, name.as_ref());
    fs::create_dir(&destination).map_err(|err| err.to_string())?;
    Ok(destination)
}

pub async fn create_folder_async(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !metadata_async(parent)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination_async(parent, name.as_ref()).await;
    create_folder_at_async(&destination).await?;
    Ok(destination)
}

pub fn create_file(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !parent.is_dir() {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination(parent, name.as_ref());
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|err| err.to_string())?;
    Ok(destination)
}

pub async fn create_file_async(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !metadata_async(parent)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("current location is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = unique_destination_async(parent, name.as_ref()).await;
    create_file_at_async(&destination).await?;
    Ok(destination)
}

pub async fn create_folder_at_async(path: &Path) -> Result<(), String> {
    compio::fs::create_dir(path)
        .await
        .map_err(|err| err.to_string())
}

pub async fn create_file_at_async(path: &Path) -> Result<(), String> {
    let mut options = compio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let file = options.open(path).await.map_err(|err| err.to_string())?;
    file.close().await.map_err(|err| err.to_string())
}

pub fn write_unique_file(
    parent: &Path,
    base_name: &str,
    extension: &str,
    data: &[u8],
) -> Result<PathBuf, String> {
    if !parent.is_dir() {
        return Err("current location is not a folder".to_string());
    }
    let extension = extension.trim_start_matches('.');
    let file_name = if extension.is_empty() {
        sanitize_child_name(base_name)?
    } else {
        sanitize_child_name(&format!("{base_name}.{extension}"))?
    };
    let destination = unique_destination(parent, file_name.as_ref());
    fs::write(&destination, data).map_err(|err| err.to_string())?;
    Ok(destination)
}

pub fn rename_path(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    if !path_exists(path) {
        return Err("item no longer exists".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "item has no parent folder".to_string())?;
    let new_name = sanitize_child_name(new_name)?;
    let destination = parent.join(new_name);
    if destination == path {
        return Ok(destination);
    }
    if path_exists(&destination) {
        return Err("an item with that name already exists".to_string());
    }
    fs::rename(path, &destination).map_err(|err| err.to_string())?;
    Ok(destination)
}

pub async fn rename_path_async(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    if !path_exists_async(path).await {
        return Err("item no longer exists".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "item has no parent folder".to_string())?;
    let new_name = sanitize_child_name(new_name)?;
    let destination = parent.join(new_name);
    if destination == path {
        return Ok(destination);
    }
    if path_exists_async(&destination).await {
        return Err("an item with that name already exists".to_string());
    }
    rename_path_to_async(path, &destination).await?;
    Ok(destination)
}

pub async fn rename_path_to_async(source: &Path, destination: &Path) -> Result<(), String> {
    compio::fs::rename(source, destination)
        .await
        .map_err(|err| err.to_string())
}

pub fn undo_create_folder(path: &Path) -> Result<String, String> {
    if !path_exists(path) {
        return Err("created folder no longer exists".to_string());
    }
    if !path.is_dir() {
        return Err("created item is no longer a folder".to_string());
    }
    fs::remove_dir(path).map_err(|err| err.to_string())?;
    Ok(format!("removed {}", path.display()))
}

pub fn undo_create_file(path: &Path) -> Result<String, String> {
    if !path_exists(path) {
        return Err("created file no longer exists".to_string());
    }
    if !path.is_file() {
        return Err("created item is no longer a file".to_string());
    }
    fs::remove_file(path).map_err(|err| err.to_string())?;
    Ok(format!("removed {}", path.display()))
}

pub fn undo_rename(original_path: &Path, renamed_path: &Path) -> Result<String, String> {
    if !path_exists(renamed_path) {
        return Err("renamed item no longer exists".to_string());
    }
    if path_exists(original_path) {
        return Err("original name is already occupied".to_string());
    }
    fs::rename(renamed_path, original_path).map_err(|err| err.to_string())?;
    Ok(format!("restored {}", original_path.display()))
}

pub fn trash_paths(paths: &[PathBuf]) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match trash_path(path) {
            Ok(destination) => summary.successes.push(TrashRecord {
                original_path: path.clone(),
                trash_path: destination,
            }),
            Err(err) => summary.failures.push(format!("{}: {err}", path.display())),
        }
    }
    summary
}

pub async fn trash_paths_async(paths: Vec<PathBuf>) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match trash_path_async(&path).await {
            Ok(destination) => summary.successes.push(TrashRecord {
                original_path: path,
                trash_path: destination,
            }),
            Err(err) => summary.failures.push(format!("{}: {err}", path.display())),
        }
    }
    summary
}

pub fn trash_path(path: &Path) -> Result<PathBuf, String> {
    if !path_exists(path) {
        return Err("item no longer exists".to_string());
    }
    if is_in_trash_files_dir(path) {
        return Err("item is already in Trash".to_string());
    }

    ensure_trash_dirs()?;
    let files_dir = trash_files_dir();
    let info_dir = trash_info_dir();

    let file_name = path
        .file_name()
        .ok_or_else(|| "item has no file name".to_string())?;
    let destination = unique_destination(&files_dir, file_name);
    move_path(path, &destination, None, &mut |_| {}).map_err(|err| err.to_string())?;

    let info_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "trash destination has invalid name".to_string())?;
    fs::write(
        info_dir.join(format!("{info_name}.trashinfo")),
        trashinfo(path),
    )
    .map_err(|err| err.to_string())?;
    let _ = set_trash_status_empty(false);
    Ok(destination)
}

pub async fn trash_path_async(path: &Path) -> Result<PathBuf, String> {
    if !path_exists_async(path).await {
        return Err("item no longer exists".to_string());
    }
    if is_in_trash_files_dir(path) {
        return Err("item is already in Trash".to_string());
    }

    ensure_trash_dirs_async().await?;
    let files_dir = trash_files_dir();
    let info_dir = trash_info_dir();

    let file_name = path
        .file_name()
        .ok_or_else(|| "item has no file name".to_string())?;
    let destination = unique_destination_async(&files_dir, file_name).await;
    move_path_async(path, &destination, None, &mut |_| {})
        .await
        .map_err(|err| err.to_string())?;

    let info_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "trash destination has invalid name".to_string())?;
    write_file_async(
        &info_dir.join(format!("{info_name}.trashinfo")),
        trashinfo(path),
    )
    .await
    .map_err(|err| err.to_string())?;
    let _ = set_trash_status_empty_async(false).await;
    Ok(destination)
}

pub fn ensure_trash_dirs() -> Result<(), String> {
    fs::create_dir_all(trash_files_dir()).map_err(|err| err.to_string())?;
    fs::create_dir_all(trash_info_dir()).map_err(|err| err.to_string())
}

async fn ensure_trash_dirs_async() -> Result<(), String> {
    create_dir_all_async(&trash_files_dir())
        .await
        .map_err(|err| err.to_string())?;
    create_dir_all_async(&trash_info_dir())
        .await
        .map_err(|err| err.to_string())
}

pub fn trash_files_dir() -> PathBuf {
    trash_home().join("files")
}

pub fn trash_info_dir() -> PathBuf {
    trash_home().join("info")
}

pub fn trashrc_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("trashrc")
}

pub fn trash_has_items() -> bool {
    fs::read_dir(trash_files_dir())
        .ok()
        .is_some_and(|mut entries| entries.any(|entry| entry.is_ok()))
}

pub fn trash_status_empty() -> bool {
    trash_status_empty_at(&trashrc_path())
}

pub fn set_trash_status_empty(empty: bool) -> Result<(), String> {
    write_trash_status_empty_at(&trashrc_path(), empty)
}

async fn set_trash_status_empty_async(empty: bool) -> Result<(), String> {
    write_trash_status_empty_at_async(&trashrc_path(), empty).await
}

pub fn sync_trash_status_empty() -> Result<bool, String> {
    let empty = !trash_has_items();
    set_trash_status_empty(empty)?;
    Ok(empty)
}

pub fn is_trash_files_dir(path: &Path) -> bool {
    path == trash_files_dir()
}

pub fn is_in_trash_files_dir(path: &Path) -> bool {
    path.starts_with(trash_files_dir())
}

pub fn undo_trash(items: &[(PathBuf, PathBuf)]) -> Result<String, String> {
    if items.is_empty() {
        return Err("no trash entries to restore".to_string());
    }

    for (original_path, trash_path) in items {
        if !path_exists(trash_path) {
            return Err(format!(
                "trash item no longer exists: {}",
                trash_path.display()
            ));
        }
        if path_exists(original_path) {
            return Err(format!(
                "original location is already occupied: {}",
                original_path.display()
            ));
        }
    }

    for (original_path, trash_path) in items {
        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        move_path(trash_path, original_path, None, &mut |_| {}).map_err(|err| err.to_string())?;
        let _ = remove_trashinfo(trash_path);
    }
    let _ = sync_trash_status_empty();

    Ok(format!("restored {} item(s)", items.len()))
}

pub fn restore_trash_paths_with_policy(
    paths: &[PathBuf],
    conflict_policy: TrashRestoreConflictPolicy,
) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match restore_trash_path(path, conflict_policy) {
            Ok(record) => summary.successes.push(record),
            Err(TrashRestoreError::Conflict(conflict)) => {
                summary.restore_conflicts.push(conflict);
            }
            Err(TrashRestoreError::Failure(err)) => {
                summary.failures.push(format!("{}: {err}", path.display()));
            }
        }
    }
    if !summary.successes.is_empty() {
        let _ = sync_trash_status_empty();
    }
    summary
}

pub fn permanently_delete_trash_paths(paths: &[PathBuf]) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match permanently_delete_trash_path(path) {
            Ok(record) => summary.successes.push(record),
            Err(err) => summary.failures.push(format!("{}: {err}", path.display())),
        }
    }
    if !summary.successes.is_empty() {
        let _ = sync_trash_status_empty();
    }
    summary
}

pub fn empty_trash() -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    let files_dir = trash_files_dir();
    if !path_exists(&files_dir) {
        let _ = set_trash_status_empty(true);
        return summary;
    }

    let entries = match fs::read_dir(&files_dir) {
        Ok(entries) => entries,
        Err(err) => {
            summary
                .failures
                .push(format!("{}: {err}", files_dir.display()));
            return summary;
        }
    };

    for entry in entries {
        match entry {
            Ok(entry) => match permanently_delete_trash_path(&entry.path()) {
                Ok(record) => summary.successes.push(record),
                Err(err) => summary
                    .failures
                    .push(format!("{}: {err}", entry.path().display())),
            },
            Err(err) => summary.failures.push(format!("trash entry: {err}")),
        }
    }
    remove_orphan_trashinfo_files(&mut summary);
    let _ = sync_trash_status_empty();
    summary
}

pub async fn empty_trash_async() -> FileActionSummary {
    empty_trash_in_dirs_async(trash_files_dir(), trash_info_dir(), trashrc_path()).await
}

async fn empty_trash_in_dirs_async(
    files_dir: PathBuf,
    info_dir: PathBuf,
    trashrc_path: PathBuf,
) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    if !path_exists_async(&files_dir).await {
        let _ = write_trash_status_empty_at_async(&trashrc_path, true).await;
        return summary;
    }

    let entries = match read_dir_entries_async(&files_dir).await {
        Ok(entries) => entries,
        Err(err) => {
            summary
                .failures
                .push(format!("{}: {err}", files_dir.display()));
            return summary;
        }
    };

    for (entry_path, _) in entries {
        match permanently_delete_trash_path_in_dirs_async(&entry_path, &files_dir, &info_dir).await
        {
            Ok(record) => summary.successes.push(record),
            Err(err) => summary
                .failures
                .push(format!("{}: {err}", entry_path.display())),
        }
    }
    remove_orphan_trashinfo_files_in_dir_async(&info_dir, &mut summary).await;
    let empty = read_dir_entries_async(&files_dir)
        .await
        .ok()
        .is_none_or(|entries| entries.is_empty());
    let _ = write_trash_status_empty_at_async(&trashrc_path, empty).await;
    summary
}

fn restore_trash_path(
    trash_path: &Path,
    conflict_policy: TrashRestoreConflictPolicy,
) -> Result<TrashRecord, TrashRestoreError> {
    if is_trash_files_dir(trash_path) || !is_in_trash_files_dir(trash_path) {
        return Err(TrashRestoreError::Failure(
            "item is not inside Trash".to_string(),
        ));
    }
    if !path_exists(trash_path) {
        return Err(TrashRestoreError::Failure(
            "trash item no longer exists".to_string(),
        ));
    }

    let original_path = trash_original_path(trash_path).map_err(TrashRestoreError::Failure)?;
    if original_path == trash_path {
        return Err(TrashRestoreError::Failure(
            "original location matches trash item".to_string(),
        ));
    }

    let overwrite_backup = if path_exists(&original_path) {
        match conflict_policy {
            TrashRestoreConflictPolicy::Skip => {
                return Err(TrashRestoreError::Conflict(TrashRestoreConflict {
                    original_path,
                    trash_path: trash_path.to_path_buf(),
                }));
            }
            TrashRestoreConflictPolicy::Replace => Some(
                backup_existing_destination(&original_path).map_err(TrashRestoreError::Failure)?,
            ),
        }
    } else {
        None
    };

    let restore_backup = |err: String| {
        if let Some(backup) = overwrite_backup.as_ref() {
            let _ = restore_overwrite_backup(&original_path, backup);
        }
        TrashRestoreError::Failure(err)
    };

    if let Some(parent) = original_path.parent() {
        fs::create_dir_all(parent).map_err(|err| restore_backup(err.to_string()))?;
    }
    move_path(trash_path, &original_path, None, &mut |_| {})
        .map_err(|err| restore_backup(err.to_string()))?;
    let _ = remove_trashinfo(trash_path);
    if let Some(backup) = overwrite_backup {
        let _ = cleanup_overwrite_backup(&backup);
    }

    Ok(TrashRecord {
        original_path,
        trash_path: trash_path.to_path_buf(),
    })
}

#[derive(Debug, Eq, PartialEq)]
enum TrashRestoreError {
    Conflict(TrashRestoreConflict),
    Failure(String),
}

fn permanently_delete_trash_path(trash_path: &Path) -> Result<TrashRecord, String> {
    if is_trash_files_dir(trash_path) || !is_in_trash_files_dir(trash_path) {
        return Err("item is not inside Trash".to_string());
    }
    if !path_exists(trash_path) {
        return Err("trash item no longer exists".to_string());
    }

    let original_path =
        trash_original_path(trash_path).unwrap_or_else(|_| trash_path.to_path_buf());
    remove_path(trash_path).map_err(|err| err.to_string())?;
    let _ = remove_trashinfo(trash_path);

    Ok(TrashRecord {
        original_path,
        trash_path: trash_path.to_path_buf(),
    })
}

async fn permanently_delete_trash_path_in_dirs_async(
    trash_path: &Path,
    files_dir: &Path,
    info_dir: &Path,
) -> Result<TrashRecord, String> {
    if trash_path == files_dir || !trash_path.starts_with(files_dir) {
        return Err("item is not inside Trash".to_string());
    }
    if !path_exists_async(trash_path).await {
        return Err("trash item no longer exists".to_string());
    }

    let original_path = trash_original_path_in_dir_async(trash_path, info_dir)
        .await
        .unwrap_or_else(|_| trash_path.to_path_buf());
    remove_path_async(trash_path)
        .await
        .map_err(|err| err.to_string())?;
    let _ = remove_trashinfo_in_dir_async(trash_path, info_dir).await;

    Ok(TrashRecord {
        original_path,
        trash_path: trash_path.to_path_buf(),
    })
}

#[derive(Debug, Default)]
pub struct FileActionSummary {
    pub successes: Vec<TrashRecord>,
    pub failures: Vec<String>,
    pub restore_conflicts: Vec<TrashRestoreConflict>,
}

#[derive(Clone, Debug)]
pub struct TrashRecord {
    pub original_path: PathBuf,
    pub trash_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashRestoreConflict {
    pub original_path: PathBuf,
    pub trash_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrashRestoreConflictPolicy {
    Skip,
    Replace,
}

pub fn trash_restore_conflict(trash_path: &Path) -> Result<Option<TrashRestoreConflict>, String> {
    if is_trash_files_dir(trash_path) || !is_in_trash_files_dir(trash_path) {
        return Err("item is not inside Trash".to_string());
    }
    if !path_exists(trash_path) {
        return Err("trash item no longer exists".to_string());
    }
    let original_path = trash_original_path(trash_path)?;
    Ok(path_exists(&original_path).then(|| TrashRestoreConflict {
        original_path,
        trash_path: trash_path.to_path_buf(),
    }))
}

fn move_path(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let bytes_total = path_size(source).unwrap_or_default();
    match fs::rename(source, destination) {
        Ok(()) => {
            progress(TransferProgress {
                bytes_done: bytes_total,
                bytes_total,
            });
            Ok(())
        }
        Err(err) if err.raw_os_error() == Some(18) => {
            let result = copy_path(source, destination, cancel, progress);
            if let Err(err) = &result
                && err.kind() == io::ErrorKind::Interrupted
            {
                let _ = remove_path(destination);
            }
            result?;
            ensure_not_cancelled(cancel)?;
            if let Err(err) = remove_path(source) {
                let _ = remove_path(destination);
                Err(err)
            } else {
                Ok(())
            }
        }
        Err(err) => Err(err),
    }
}

fn copy_path(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let destination_preexisting = fs::symlink_metadata(destination).is_ok();
    let bytes_total = path_size(source)?;
    let mut bytes_done = 0;
    let result = copy_path_inner(
        source,
        destination,
        cancel,
        &mut bytes_done,
        bytes_total,
        progress,
    );

    if result.is_err() && !destination_preexisting {
        let _ = remove_path_if_present(destination);
    }

    result
}

fn copy_path_inner(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        copy_symlink(
            source,
            destination,
            cancel,
            bytes_done,
            bytes_total,
            metadata.len(),
            progress,
        )
    } else if metadata.is_dir() {
        copy_directory(
            source,
            destination,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )
    } else {
        copy_file(
            source,
            destination,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )
    }
}

fn copy_symlink(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    link_size: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let target = fs::read_link(source)?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, destination)?;
    }

    #[cfg(not(unix))]
    {
        let metadata = fs::metadata(source)?;
        if metadata.is_dir() {
            std::os::windows::fs::symlink_dir(&target, destination)?;
        } else {
            std::os::windows::fs::symlink_file(&target, destination)?;
        }
    }

    *bytes_done = bytes_done.saturating_add(link_size).min(bytes_total);
    progress(TransferProgress {
        bytes_done: *bytes_done,
        bytes_total,
    });
    Ok(())
}

fn copy_directory(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    fs::create_dir(destination)?;
    let metadata = fs::metadata(source)?;
    fs::set_permissions(destination, metadata.permissions())?;

    for entry in fs::read_dir(source)? {
        ensure_not_cancelled(cancel)?;
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        copy_path_inner(
            &source_path,
            &destination_path,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )?;
    }

    Ok(())
}

fn copy_file(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    let mut reader = fs::File::open(source)?;
    let mut writer = fs::File::create(destination)?;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        ensure_not_cancelled(cancel)?;
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        *bytes_done = bytes_done.saturating_add(read as u64);
        progress(TransferProgress {
            bytes_done: *bytes_done,
            bytes_total,
        });
    }

    if let Ok(metadata) = fs::metadata(source) {
        let _ = fs::set_permissions(destination, metadata.permissions());
    }
    Ok(())
}

async fn move_path_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let bytes_total = path_size_async(source).await.unwrap_or_default();
    match compio::fs::rename(source, destination).await {
        Ok(()) => {
            progress(TransferProgress {
                bytes_done: bytes_total,
                bytes_total,
            });
            Ok(())
        }
        Err(err) if err.raw_os_error() == Some(18) => {
            let result = copy_path_async(source, destination, cancel, progress).await;
            if let Err(err) = &result
                && err.kind() == io::ErrorKind::Interrupted
            {
                let _ = remove_path_async(destination).await;
            }
            result?;
            ensure_not_cancelled(cancel)?;
            if let Err(err) = remove_path_async(source).await {
                let _ = remove_path_async(destination).await;
                Err(err)
            } else {
                Ok(())
            }
        }
        Err(err) => Err(err),
    }
}

async fn copy_path_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let (destination_preexisting, bytes_total) =
        copy_path_preflight_async(source, destination).await?;
    let mut bytes_done = 0;
    let result = copy_path_inner_async(
        source,
        destination,
        cancel,
        &mut bytes_done,
        bytes_total,
        progress,
    )
    .await;

    if result.is_err() && !destination_preexisting {
        let _ = remove_path_if_present_async(destination).await;
    }

    result
}

fn copy_path_inner_async<'a, P>(
    source: &'a Path,
    destination: &'a Path,
    cancel: Option<&'a OperationController>,
    bytes_done: &'a mut u64,
    bytes_total: u64,
    progress: &'a mut P,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + 'a>>
where
    P: FnMut(TransferProgress) + 'a,
{
    Box::pin(async move {
        let metadata = symlink_metadata_async(source).await?;
        if metadata.file_type().is_symlink() {
            copy_symlink_async(
                source,
                destination,
                cancel,
                bytes_done,
                bytes_total,
                metadata.len(),
                progress,
            )
            .await
        } else if metadata.is_dir() {
            copy_directory_async(
                source,
                destination,
                cancel,
                bytes_done,
                bytes_total,
                progress,
            )
            .await
        } else {
            copy_file_async(
                source,
                destination,
                cancel,
                bytes_done,
                bytes_total,
                progress,
            )
            .await
        }
    })
}

async fn copy_symlink_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    link_size: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    ensure_not_cancelled(cancel)?;
    let target = read_link_async(source).await?;

    #[cfg(unix)]
    {
        compio::fs::symlink(&target, destination).await?;
    }

    #[cfg(not(unix))]
    {
        let metadata = metadata_async(source).await?;
        if metadata.is_dir() {
            compio::fs::symlink_dir(&target, destination).await?;
        } else {
            compio::fs::symlink_file(&target, destination).await?;
        }
    }

    *bytes_done = bytes_done.saturating_add(link_size).min(bytes_total);
    progress(TransferProgress {
        bytes_done: *bytes_done,
        bytes_total,
    });
    Ok(())
}

async fn copy_directory_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    compio::fs::create_dir(destination).await?;
    if let Ok(metadata) = compio::fs::metadata(source).await {
        let _ = compio::fs::set_permissions(destination, metadata.permissions()).await;
    }

    for (source_path, file_name) in read_dir_entries_async(source).await? {
        ensure_not_cancelled(cancel)?;
        let destination_path = destination.join(file_name);
        copy_path_inner_async(
            &source_path,
            &destination_path,
            cancel,
            bytes_done,
            bytes_total,
            progress,
        )
        .await?;
    }

    Ok(())
}

async fn copy_file_async(
    source: &Path,
    destination: &Path,
    cancel: Option<&OperationController>,
    bytes_done: &mut u64,
    bytes_total: u64,
    progress: &mut impl FnMut(TransferProgress),
) -> io::Result<()> {
    let from_file = compio::fs::File::open(source).await?;
    let mut to_file = compio::fs::File::create(destination).await?;
    if let Ok(metadata) = compio::fs::metadata(source).await {
        let _ = to_file.set_permissions(metadata.permissions()).await;
    }

    let mut buffer = vec![0_u8; 64 * 1024];
    let mut position = 0_u64;

    loop {
        ensure_not_cancelled(cancel)?;
        let BufResult(read_result, read_buffer) = from_file.read_at(buffer, position).await;
        let read = read_result?;
        if read == 0 {
            break;
        }
        let BufResult(write_result, write_buffer) = to_file
            .write_all_at(compio::buf::IoBuf::slice(read_buffer, ..read), position)
            .await;
        buffer = write_buffer.into_inner();
        write_result?;
        *bytes_done = bytes_done.saturating_add(read as u64);
        position = position.saturating_add(read as u64);
        progress(TransferProgress {
            bytes_done: *bytes_done,
            bytes_total,
        });
    }

    let _ = from_file.close().await;
    let _ = to_file.close().await;
    Ok(())
}

fn link_path(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, destination)
    }

    #[cfg(not(unix))]
    {
        let metadata = fs::metadata(source)?;
        if metadata.is_dir() {
            std::os::windows::fs::symlink_dir(source, destination)
        } else {
            std::os::windows::fs::symlink_file(source, destination)
        }
    }
}

async fn link_path_async(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        compio::fs::symlink(source, destination).await
    }

    #[cfg(not(unix))]
    {
        let metadata = metadata_async(source).await?;
        if metadata.is_dir() {
            compio::fs::symlink_dir(source, destination).await
        } else {
            compio::fs::symlink_file(source, destination).await
        }
    }
}

async fn copy_path_preflight_async(source: &Path, destination: &Path) -> io::Result<(bool, u64)> {
    Ok((
        symlink_metadata_async(destination).await.is_ok(),
        path_size_async(source).await?,
    ))
}

fn path_size_async<'a>(
    path: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<u64>> + 'a>> {
    Box::pin(async move {
        let metadata = symlink_metadata_async(path).await?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let mut total: u64 = 0;
            for (entry_path, _) in read_dir_entries_async(path).await? {
                total = total.saturating_add(path_size_async(&entry_path).await?);
            }
            Ok(total)
        } else {
            Ok(metadata.len())
        }
    })
}

async fn symlink_metadata_async(path: &Path) -> io::Result<compio::fs::Metadata> {
    compio::fs::symlink_metadata(path).await
}

async fn metadata_async(path: &Path) -> io::Result<compio::fs::Metadata> {
    compio::fs::metadata(path).await
}

async fn read_link_async(path: &Path) -> io::Result<PathBuf> {
    let path = path.to_path_buf();
    compio_blocking_io(move || fs::read_link(path)).await
}

async fn read_file_to_string_async(path: &Path) -> io::Result<String> {
    let path = path.to_path_buf();
    compio_blocking_io(move || fs::read_to_string(path)).await
}

async fn read_dir_entries_async(path: &Path) -> io::Result<Vec<(PathBuf, OsString)>> {
    let path = path.to_path_buf();
    compio_blocking_io(move || {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            entries.push((entry.path(), entry.file_name()));
        }
        Ok(entries)
    })
    .await
}

async fn canonicalize_async(path: &Path) -> io::Result<PathBuf> {
    let path = path.to_path_buf();
    compio_blocking_io(move || path.canonicalize()).await
}

async fn compio_blocking_io<T>(
    task: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T>
where
    T: Send + 'static,
{
    compio::runtime::spawn_blocking(task)
        .await
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?
}

fn create_dir_all_async<'a>(
    path: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + 'a>> {
    Box::pin(async move {
        if path.as_os_str().is_empty() {
            return Ok(());
        }
        if metadata_async(path)
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            return Ok(());
        }
        if let Some(parent) = path.parent().filter(|parent| *parent != path) {
            create_dir_all_async(parent).await?;
        }
        match compio::fs::create_dir(path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if metadata_async(path)
                    .await
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false)
                {
                    Ok(())
                } else {
                    Err(err)
                }
            }
            Err(err) => Err(err),
        }
    })
}

async fn write_file_async(path: &Path, contents: String) -> io::Result<()> {
    let mut file = compio::fs::File::create(path).await?;
    let BufResult(write_result, file_contents) = file.write_all_at(contents.into_bytes(), 0).await;
    write_result?;
    drop(file_contents);
    file.close().await
}

async fn remove_path_async(path: &Path) -> io::Result<()> {
    let metadata = symlink_metadata_async(path).await?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        remove_dir_all_async(path).await
    } else {
        compio::fs::remove_file(path).await
    }
}

async fn remove_path_if_present_async(path: &Path) -> io::Result<()> {
    match symlink_metadata_async(path).await {
        Ok(_) => remove_path_async(path).await,
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn remove_dir_all_async<'a>(
    path: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + 'a>> {
    Box::pin(async move {
        for (entry_path, _) in read_dir_entries_async(path).await? {
            remove_path_async(&entry_path).await?;
        }
        compio::fs::remove_dir(path).await
    })
}

async fn path_exists_async(path: &Path) -> bool {
    symlink_metadata_async(path).await.is_ok()
}

async fn transfer_destination_async(
    source: &Path,
    target_dir: &Path,
    file_name: &std::ffi::OsStr,
    conflict_policy: &str,
) -> Result<PathBuf, String> {
    if let Some(name) = conflict_policy.strip_prefix("rename:") {
        return renamed_destination_async(target_dir, name).await;
    }

    let base = target_dir.join(file_name);
    match conflict_policy {
        "keep-both" => Ok(unique_destination_async(target_dir, file_name).await),
        "overwrite" => {
            if base == source {
                return Err("cannot overwrite an item with itself".to_string());
            }
            Ok(base)
        }
        _ => Err(format!("unknown conflict policy: {conflict_policy}")),
    }
}

async fn renamed_destination_async(target_dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !metadata_async(target_dir)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err("target is not a folder".to_string());
    }
    let name = sanitize_child_name(name)?;
    let destination = target_dir.join(name);
    if path_exists_async(&destination).await {
        return Err("an item with that name already exists".to_string());
    }
    Ok(destination)
}

async fn unique_destination_async(target_dir: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let initial = target_dir.join(file_name);
    if !path_exists_async(&initial).await {
        return initial;
    }

    let source_name = Path::new(file_name);
    let stem = source_name
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("item");
    let extension = source_name
        .extension()
        .and_then(|extension| extension.to_str());

    for index in 1.. {
        let suffix = if index == 1 {
            "copy".to_string()
        } else {
            format!("copy {index}")
        };
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} {suffix}.{extension}"),
            _ => format!("{stem} {suffix}"),
        };
        let candidate = target_dir.join(candidate_name);
        if !path_exists_async(&candidate).await {
            return candidate;
        }
    }

    unreachable!("unbounded destination search should always return")
}

async fn backup_existing_destination_async(destination: &Path) -> Result<PathBuf, String> {
    let backup = overwrite_backup_path_async(destination).await?;
    compio::fs::rename(destination, &backup)
        .await
        .map_err(|err| err.to_string())?;
    Ok(backup)
}

async fn restore_overwrite_backup_async(destination: &Path, backup: &Path) -> Result<(), String> {
    if path_exists_async(destination).await {
        remove_path_async(destination)
            .await
            .map_err(|err| err.to_string())?;
    }
    compio::fs::rename(backup, destination)
        .await
        .map_err(|err| err.to_string())
}

async fn overwrite_backup_path_async(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "overwrite target has no parent folder".to_string())?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("item");
    let pid = std::process::id();
    for index in 0.. {
        let candidate = parent.join(format!(".{name}.fika-overwrite-backup-{pid}-{index}"));
        if !path_exists_async(&candidate).await {
            return Ok(candidate);
        }
    }
    unreachable!("unbounded overwrite backup search should always return")
}

fn path_size(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        let mut total: u64 = 0;
        for entry in fs::read_dir(path)? {
            total = total.saturating_add(path_size(&entry?.path())?);
        }
        Ok(total)
    } else {
        Ok(metadata.len())
    }
}

fn ensure_not_cancelled(cancel: Option<&OperationController>) -> io::Result<()> {
    if cancel.is_some_and(OperationController::is_cancelled) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "operation cancelled",
        ))
    } else {
        Ok(())
    }
}

fn remove_path(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn remove_path_if_present(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => remove_path(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn transfer_destination(
    source: &Path,
    target_dir: &Path,
    file_name: &std::ffi::OsStr,
    conflict_policy: &str,
) -> Result<PathBuf, String> {
    if let Some(name) = conflict_policy.strip_prefix("rename:") {
        return renamed_destination(target_dir, name);
    }

    let base = target_dir.join(file_name);
    match conflict_policy {
        "keep-both" => Ok(unique_destination(target_dir, file_name)),
        "overwrite" => {
            if base == source {
                return Err("cannot overwrite an item with itself".to_string());
            }
            Ok(base)
        }
        _ => Err(format!("unknown conflict policy: {conflict_policy}")),
    }
}

fn backup_existing_destination(destination: &Path) -> Result<PathBuf, String> {
    let backup = overwrite_backup_path(destination)?;
    fs::rename(destination, &backup).map_err(|err| err.to_string())?;
    Ok(backup)
}

pub fn cleanup_overwrite_backup(backup: &Path) -> Result<(), String> {
    if path_exists(backup) {
        remove_path(backup).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn restore_overwrite_backup(destination: &Path, backup: &Path) -> Result<(), String> {
    if path_exists(destination) {
        remove_path(destination).map_err(|err| err.to_string())?;
    }
    fs::rename(backup, destination).map_err(|err| err.to_string())
}

fn remove_trashinfo(trash_path: &Path) -> Result<(), String> {
    let Some(info_path) = trash_info_path(trash_path) else {
        return Ok(());
    };
    if path_exists(&info_path) {
        fs::remove_file(info_path).map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn remove_trashinfo_in_dir_async(trash_path: &Path, info_dir: &Path) -> Result<(), String> {
    let Some(info_path) = trash_info_path_in_dir(trash_path, info_dir) else {
        return Ok(());
    };
    if path_exists_async(&info_path).await {
        compio::fs::remove_file(&info_path)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn remove_orphan_trashinfo_files(summary: &mut FileActionSummary) {
    let info_dir = trash_info_dir();
    let Ok(entries) = fs::read_dir(&info_dir) else {
        return;
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) == Some("trashinfo")
                    && let Err(err) = fs::remove_file(&path)
                {
                    summary.failures.push(format!("{}: {err}", path.display()));
                }
            }
            Err(err) => summary
                .failures
                .push(format!("trash metadata entry: {err}")),
        }
    }
}

async fn remove_orphan_trashinfo_files_in_dir_async(
    info_dir: &Path,
    summary: &mut FileActionSummary,
) {
    let Ok(entries) = read_dir_entries_async(info_dir).await else {
        return;
    };
    for (path, _) in entries {
        if path.extension().and_then(|extension| extension.to_str()) == Some("trashinfo")
            && let Err(err) = compio::fs::remove_file(&path).await
        {
            summary.failures.push(format!("{}: {err}", path.display()));
        }
    }
}

fn trash_original_path(trash_path: &Path) -> Result<PathBuf, String> {
    trash_metadata(trash_path).map(|metadata| metadata.original_path)
}

async fn trash_original_path_in_dir_async(
    trash_path: &Path,
    info_dir: &Path,
) -> Result<PathBuf, String> {
    trash_metadata_in_dir_async(trash_path, info_dir)
        .await
        .map(|metadata| metadata.original_path)
}

pub fn trash_metadata(trash_path: &Path) -> Result<TrashMetadata, String> {
    let info_path =
        trash_info_path(trash_path).ok_or_else(|| "trash item has no metadata name".to_string())?;
    let contents = fs::read_to_string(&info_path).map_err(|err| {
        format!(
            "failed to read trash metadata {}: {err}",
            info_path.display()
        )
    })?;
    trash_metadata_from_info(&contents)
}

async fn trash_metadata_in_dir_async(
    trash_path: &Path,
    info_dir: &Path,
) -> Result<TrashMetadata, String> {
    let info_path = trash_info_path_in_dir(trash_path, info_dir)
        .ok_or_else(|| "trash item has no metadata name".to_string())?;
    let contents = read_file_to_string_async(&info_path).await.map_err(|err| {
        format!(
            "failed to read trash metadata {}: {err}",
            info_path.display()
        )
    })?;
    trash_metadata_from_info(&contents)
}

#[cfg(test)]
fn trash_original_path_from_info(contents: &str) -> Result<PathBuf, String> {
    trash_metadata_from_info(contents).map(|metadata| metadata.original_path)
}

fn trash_metadata_from_info(contents: &str) -> Result<TrashMetadata, String> {
    let encoded = contents
        .lines()
        .find_map(|line| line.strip_prefix("Path="))
        .ok_or_else(|| "trash metadata is missing Path".to_string())?;
    let path = percent_decode_path(encoded)?;
    if !path.is_absolute() {
        return Err(format!(
            "trash metadata Path is not absolute: {}",
            path.display()
        ));
    }
    let deletion_date = contents
        .lines()
        .find_map(|line| line.strip_prefix("DeletionDate="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(TrashMetadata {
        original_path: path,
        deletion_date,
    })
}

fn trash_info_path(trash_path: &Path) -> Option<PathBuf> {
    trash_info_path_in_dir(trash_path, &trash_info_dir())
}

fn trash_info_path_in_dir(trash_path: &Path, info_dir: &Path) -> Option<PathBuf> {
    let name = trash_path.file_name()?.to_str()?;
    Some(info_dir.join(format!("{name}.trashinfo")))
}

fn overwrite_backup_path(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "overwrite target has no parent folder".to_string())?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("item");
    let pid = std::process::id();
    for index in 0.. {
        let candidate = parent.join(format!(".{name}.fika-overwrite-backup-{pid}-{index}"));
        if !path_exists(&candidate) {
            return Ok(candidate);
        }
    }
    unreachable!("unbounded overwrite backup search should always return")
}

fn sanitize_child_name(name: &str) -> Result<std::ffi::OsString, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.contains('/') || name == "." || name == ".." {
        return Err("name must not contain path separators".to_string());
    }
    Ok(name.into())
}

fn trash_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Trash")
}

fn trash_status_empty_at(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| trash_status_empty_from_contents(&contents))
        .unwrap_or(true)
}

fn write_trash_status_empty_at(path: &Path, empty: bool) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, trash_status_contents(empty)).map_err(|err| err.to_string())
}

async fn write_trash_status_empty_at_async(path: &Path, empty: bool) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_dir_all_async(parent)
            .await
            .map_err(|err| err.to_string())?;
    }
    write_file_async(path, trash_status_contents(empty))
        .await
        .map_err(|err| err.to_string())
}

fn trash_status_contents(empty: bool) -> String {
    format!("[Status]\nEmpty={}\n", if empty { "true" } else { "false" })
}

fn trash_status_empty_from_contents(contents: &str) -> Option<bool> {
    let mut in_status = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_status = line[1..line.len() - 1].trim() == "Status";
            continue;
        }
        if !in_status {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "Empty" {
            continue;
        }
        return match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        };
    }
    None
}

fn trashinfo(path: &Path) -> String {
    format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode_path(path),
        current_trash_time()
    )
}

fn current_trash_time() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let days = secs / 86_400;
    let seconds_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d as u32)
}

fn percent_encode_path(path: &Path) -> String {
    path.to_string_lossy()
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn percent_decode_path(value: &str) -> Result<PathBuf, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("trash metadata Path contains truncated percent escape".to_string());
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| "trash metadata Path contains invalid percent escape".to_string())?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| "trash metadata Path contains invalid percent escape".to_string())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded)
        .map(PathBuf::from)
        .map_err(|_| "trash metadata Path is not valid UTF-8".to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn unique_destination(target_dir: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let initial = target_dir.join(file_name);
    if !path_exists(&initial) {
        return initial;
    }

    let source_name = Path::new(file_name);
    let stem = source_name
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("item");
    let extension = source_name
        .extension()
        .and_then(|extension| extension.to_str());

    for index in 1.. {
        let suffix = if index == 1 {
            "copy".to_string()
        } else {
            format!("copy {index}")
        };
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} {suffix}.{extension}"),
            _ => format!("{stem} {suffix}"),
        };
        let candidate = target_dir.join(candidate_name);
        if !path_exists(&candidate) {
            return candidate;
        }
    }

    unreachable!("unbounded destination search should always return")
}

pub fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_reports_progress() {
        let temp = test_dir("progress");
        fs::create_dir_all(&temp).unwrap();
        let source = temp.join("source.bin");
        let target = temp.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(&source, vec![7_u8; 128 * 1024]).unwrap();

        let mut progress_events = Vec::new();
        let destination = perform_transfer_with_progress(
            "copy",
            &source,
            &target,
            "keep-both",
            None,
            |progress| {
                progress_events.push(progress);
            },
        )
        .unwrap();

        assert!(destination.exists());
        assert_eq!(fs::metadata(destination).unwrap().len(), 128 * 1024);
        assert!(progress_events.last().is_some_and(
            |progress| progress.bytes_done == 128 * 1024 && progress.bytes_total == 128 * 1024
        ));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn async_copy_reports_progress_via_compio() {
        let temp = test_dir("async-progress");
        fs::create_dir_all(&temp).unwrap();
        let source = temp.join("source.bin");
        let target = temp.join("target");
        fs::create_dir(&target).unwrap();
        let payload = vec![11_u8; 96 * 1024];
        fs::write(&source, &payload).unwrap();
        let controller = OperationController::new();

        let outcome =
            futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
                let source = source.clone();
                let target = target.clone();
                let controller = controller.clone();
                move || async move {
                    let progress_controller = controller.clone();
                    perform_transfer_with_progress_outcome_async(
                        "copy",
                        &source,
                        &target,
                        "keep-both",
                        Some(controller),
                        move |progress| {
                            progress_controller.set_progress(progress);
                        },
                    )
                    .await
                }
            }))
            .unwrap()
            .unwrap();

        assert_eq!(fs::read(outcome.destination).unwrap(), payload);
        let progress = controller.progress();
        assert_eq!(progress.bytes_done, 96 * 1024);
        assert_eq!(progress.bytes_total, 96 * 1024);
        assert_eq!(controller.progress(), progress);
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn create_file_async_uses_compio_unique_destination() {
        let temp = test_dir("create-file-async");
        fs::create_dir_all(&temp).unwrap();
        fs::write(temp.join("New File.txt"), b"occupied").unwrap();

        let created =
            futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
                let temp = temp.clone();
                move || async move { create_file_async(&temp, "New File.txt").await }
            }))
            .unwrap()
            .unwrap();

        assert_eq!(created.file_name().unwrap(), "New File copy.txt");
        assert!(created.is_file());
        assert_eq!(fs::read(temp.join("New File.txt")).unwrap(), b"occupied");
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn create_exact_async_helpers_use_requested_path() {
        let temp = test_dir("create-exact-async");
        fs::create_dir_all(&temp).unwrap();
        let folder = temp.join("made");
        let file = temp.join("note.txt");

        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let folder = folder.clone();
            let file = file.clone();
            move || async move {
                create_folder_at_async(&folder).await?;
                create_file_at_async(&file).await
            }
        }))
        .unwrap()
        .unwrap();

        assert!(folder.is_dir());
        assert!(file.is_file());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn rename_path_async_uses_compio_and_rejects_conflicts() {
        let temp = test_dir("rename-async");
        fs::create_dir_all(&temp).unwrap();
        let original = temp.join("old.txt");
        let occupied = temp.join("taken.txt");
        fs::write(&original, b"old").unwrap();
        fs::write(&occupied, b"taken").unwrap();

        let conflict =
            futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
                let original = original.clone();
                move || async move { rename_path_async(&original, "taken.txt").await }
            }))
            .unwrap()
            .unwrap_err();
        assert_eq!(conflict, "an item with that name already exists");

        let renamed =
            futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
                let original = original.clone();
                move || async move { rename_path_async(&original, "new.txt").await }
            }))
            .unwrap()
            .unwrap();

        assert_eq!(renamed, temp.join("new.txt"));
        assert!(!original.exists());
        assert!(renamed.is_file());
        assert_eq!(fs::read(occupied).unwrap(), b"taken");
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn copy_can_be_cancelled() {
        let temp = test_dir("cancel");
        fs::create_dir_all(&temp).unwrap();
        let source = temp.join("source.bin");
        let target = temp.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(&source, vec![11_u8; 256 * 1024]).unwrap();

        let controller = OperationController::new();
        let cancel_from_progress = controller.clone();
        let result = perform_transfer_with_progress(
            "copy",
            &source,
            &target,
            "keep-both",
            Some(controller),
            move |_| cancel_from_progress.cancel(),
        );

        assert!(result.is_err());
        assert!(!target.join("source.bin").exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn cancelled_directory_copy_removes_partial_destination_root() {
        let temp = test_dir("cancel-directory");
        fs::create_dir_all(&temp).unwrap();
        let source = temp.join("source");
        let nested = source.join("nested");
        let target = temp.join("target");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(nested.join("first.bin"), vec![13_u8; 128 * 1024]).unwrap();
        fs::write(source.join("second.bin"), vec![17_u8; 128 * 1024]).unwrap();

        let controller = OperationController::new();
        let cancel_from_progress = controller.clone();
        let result = perform_transfer_with_progress(
            "copy",
            &source,
            &target,
            "keep-both",
            Some(controller),
            move |_| cancel_from_progress.cancel(),
        );

        assert!(result.is_err());
        assert!(!target.join("source").exists());
        assert!(source.join("nested/first.bin").exists());
        assert!(source.join("second.bin").exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[cfg(unix)]
    #[test]
    fn copy_preserves_symlinks_instead_of_dereferencing() {
        let temp = test_dir("copy-symlink");
        let source_dir = temp.join("source");
        let target = temp.join("target");
        let nested_target = temp.join("nested-target");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir(&target).unwrap();
        fs::create_dir(&nested_target).unwrap();
        fs::write(source_dir.join("file.txt"), "linked file").unwrap();
        fs::create_dir(source_dir.join("folder")).unwrap();
        std::os::unix::fs::symlink("file.txt", source_dir.join("file-link")).unwrap();
        std::os::unix::fs::symlink("folder", source_dir.join("folder-link")).unwrap();

        let copied_file_link = perform_transfer_with_progress(
            "copy",
            &source_dir.join("file-link"),
            &target,
            "keep-both",
            None,
            |_| {},
        )
        .unwrap();
        let copied_folder_link = perform_transfer_with_progress(
            "copy",
            &source_dir.join("folder-link"),
            &target,
            "keep-both",
            None,
            |_| {},
        )
        .unwrap();

        assert!(
            fs::symlink_metadata(&copied_file_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&copied_file_link).unwrap(),
            PathBuf::from("file.txt")
        );
        assert!(
            fs::symlink_metadata(&copied_folder_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&copied_folder_link).unwrap(),
            PathBuf::from("folder")
        );

        let mut directory_progress = Vec::new();
        let copied_dir = perform_transfer_with_progress(
            "copy",
            &source_dir,
            &nested_target,
            "keep-both",
            None,
            |progress| directory_progress.push(progress),
        )
        .unwrap();
        assert!(!directory_progress.is_empty());
        assert!(
            directory_progress
                .iter()
                .all(|progress| progress.bytes_done <= progress.bytes_total)
        );
        assert!(
            directory_progress
                .last()
                .is_some_and(|progress| progress.bytes_done == progress.bytes_total)
        );

        let copied_nested_file_link = copied_dir.join("file-link");
        let copied_nested_folder_link = copied_dir.join("folder-link");
        assert!(
            fs::symlink_metadata(&copied_nested_file_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&copied_nested_file_link).unwrap(),
            PathBuf::from("file.txt")
        );
        assert!(
            fs::symlink_metadata(&copied_nested_folder_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&copied_nested_folder_link).unwrap(),
            PathBuf::from("folder")
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[cfg(unix)]
    #[test]
    fn copy_preserves_broken_symlink_without_dereferencing() {
        let temp = test_dir("copy-broken-symlink");
        let source_dir = temp.join("source");
        let target = temp.join("target");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir(&target).unwrap();
        let source = source_dir.join("missing-link");
        std::os::unix::fs::symlink("missing-target.txt", &source).unwrap();

        assert!(!source.exists());
        assert!(path_exists(&source));

        let copied =
            perform_transfer_with_progress("copy", &source, &target, "keep-both", None, |_| {})
                .unwrap();

        assert!(!copied.exists());
        assert!(
            fs::symlink_metadata(&copied)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&copied).unwrap(),
            PathBuf::from("missing-target.txt")
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[cfg(unix)]
    #[test]
    fn keep_both_treats_broken_symlink_destination_as_conflict() {
        let temp = test_dir("broken-symlink-conflict");
        let source_dir = temp.join("source");
        let target = temp.join("target");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir(&target).unwrap();
        let source = source_dir.join("note.txt");
        let occupied = target.join("note.txt");
        fs::write(&source, "new").unwrap();
        std::os::unix::fs::symlink("missing-target.txt", &occupied).unwrap();

        assert!(!occupied.exists());
        assert!(path_exists(&occupied));

        let copied =
            perform_transfer_with_progress("copy", &source, &target, "keep-both", None, |_| {})
                .unwrap();

        assert_eq!(copied, target.join("note copy.txt"));
        assert_eq!(fs::read_to_string(copied).unwrap(), "new");
        assert!(
            fs::symlink_metadata(&occupied)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&occupied).unwrap(),
            PathBuf::from("missing-target.txt")
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn transfer_core_rejects_self_and_descendant_targets() {
        let temp = test_dir("target-relation");
        let source = temp.join("source");
        let child = source.join("child");
        let sibling = temp.join("sibling");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&sibling).unwrap();

        assert_eq!(
            transfer_target_relation(&source, &source),
            Some(TransferTargetRelation::Same)
        );
        assert_eq!(
            transfer_target_relation(&source, &child),
            Some(TransferTargetRelation::Descendant)
        );
        assert_eq!(transfer_target_relation(&source, &sibling), None);
        assert_eq!(
            perform_transfer_with_progress("copy", &source, &source, "keep-both", None, |_| {})
                .unwrap_err(),
            "cannot transfer an item onto itself"
        );
        assert_eq!(
            perform_transfer_with_progress("copy", &source, &child, "keep-both", None, |_| {})
                .unwrap_err(),
            "cannot transfer a folder into itself"
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[cfg(unix)]
    #[test]
    fn transfer_core_rejects_symlinked_descendant_target() {
        let temp = test_dir("symlink-target-relation");
        let source = temp.join("source");
        let child = source.join("child");
        let link = temp.join("link-to-child");
        fs::create_dir_all(&child).unwrap();
        std::os::unix::fs::symlink(&child, &link).unwrap();

        assert_eq!(
            transfer_target_relation(&source, &link),
            Some(TransferTargetRelation::Descendant)
        );
        assert_eq!(
            perform_transfer_with_progress("copy", &source, &link, "keep-both", None, |_| {})
                .unwrap_err(),
            "cannot transfer a folder into itself"
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn transfer_conflict_policy_can_keep_both_overwrite_or_rename() {
        let temp = test_dir("conflict");
        fs::create_dir_all(&temp).unwrap();
        let source_dir = temp.join("source");
        let target = temp.join("target");
        fs::create_dir(&source_dir).unwrap();
        fs::create_dir(&target).unwrap();
        let source = source_dir.join("note.txt");
        fs::write(&source, b"new").unwrap();
        fs::write(target.join("note.txt"), b"old").unwrap();

        let kept =
            perform_transfer_with_progress("copy", &source, &target, "keep-both", None, |_| {})
                .unwrap();
        assert_eq!(
            kept.file_name().and_then(|name| name.to_str()),
            Some("note copy.txt")
        );
        assert_eq!(fs::read_to_string(target.join("note.txt")).unwrap(), "old");

        let overwritten =
            perform_transfer_with_progress("copy", &source, &target, "overwrite", None, |_| {})
                .unwrap();
        assert_eq!(overwritten, target.join("note.txt"));
        assert_eq!(fs::read_to_string(target.join("note.txt")).unwrap(), "new");

        let renamed = perform_transfer_with_progress(
            "copy",
            &source,
            &target,
            "rename:custom-note.txt",
            None,
            |_| {},
        )
        .unwrap();
        assert_eq!(renamed, target.join("custom-note.txt"));
        assert_eq!(
            fs::read_to_string(target.join("custom-note.txt")).unwrap(),
            "new"
        );

        let rejected_existing = perform_transfer_with_progress(
            "copy",
            &source,
            &target,
            "rename:note.txt",
            None,
            |_| {},
        );
        assert!(rejected_existing.is_err());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn overwrite_replaces_existing_directory_atomically() {
        let temp = test_dir("overwrite-dir");
        fs::create_dir_all(&temp).unwrap();
        let source_parent = temp.join("source-parent");
        let target = temp.join("target");
        let source = source_parent.join("project");
        let existing = target.join("project");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&existing).unwrap();
        fs::write(source.join("new.txt"), "new").unwrap();
        fs::write(existing.join("old.txt"), "old").unwrap();

        let overwritten =
            perform_transfer_with_progress("copy", &source, &target, "overwrite", None, |_| {})
                .unwrap();

        assert_eq!(overwritten, existing);
        assert_eq!(fs::read_to_string(existing.join("new.txt")).unwrap(), "new");
        assert!(!existing.join("old.txt").exists());
        assert!(!fs::read_dir(&target).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("fika-overwrite-backup")
        }));

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn overwrite_outcome_can_restore_replaced_item_on_undo() {
        let temp = test_dir("overwrite-undo");
        fs::create_dir_all(&temp).unwrap();
        let source_dir = temp.join("source");
        let target = temp.join("target");
        fs::create_dir(&source_dir).unwrap();
        fs::create_dir(&target).unwrap();
        let source = source_dir.join("note.txt");
        let destination = target.join("note.txt");
        fs::write(&source, "new").unwrap();
        fs::write(&destination, "old").unwrap();

        let outcome = perform_transfer_with_progress_outcome(
            "copy",
            &source,
            &target,
            "overwrite",
            None,
            |_| {},
        )
        .unwrap();
        let backup = outcome.overwritten_backup.clone().unwrap();

        assert_eq!(outcome.destination, destination);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "new");
        assert!(backup.exists());

        undo_transfer_with_backup("copy", &source, &destination, Some(&backup)).unwrap();

        assert_eq!(fs::read_to_string(&destination).unwrap(), "old");
        assert!(!backup.exists());
        assert!(source.exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_transfer_removes_copy_and_restores_move() {
        let temp = test_dir("undo");
        fs::create_dir_all(&temp).unwrap();
        let source_dir = temp.join("source");
        let target = temp.join("target");
        fs::create_dir(&source_dir).unwrap();
        fs::create_dir(&target).unwrap();

        let copy_source = source_dir.join("copy.txt");
        fs::write(&copy_source, b"copy").unwrap();
        let copied = perform_transfer_with_progress(
            "copy",
            &copy_source,
            &target,
            "keep-both",
            None,
            |_| {},
        )
        .unwrap();
        undo_transfer("copy", &copy_source, &copied).unwrap();
        assert!(!copied.exists());
        assert!(copy_source.exists());

        let move_source = source_dir.join("move.txt");
        fs::write(&move_source, b"move").unwrap();
        let moved = perform_transfer_with_progress(
            "move",
            &move_source,
            &target,
            "keep-both",
            None,
            |_| {},
        )
        .unwrap();
        assert!(!move_source.exists());
        undo_transfer("move", &move_source, &moved).unwrap();
        assert!(move_source.exists());
        assert!(!moved.exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_create_folder_removes_empty_created_folder() {
        let temp = test_dir("undo-create-folder");
        fs::create_dir_all(&temp).unwrap();
        let created = create_folder(&temp, "New Folder").unwrap();

        undo_create_folder(&created).unwrap();

        assert!(!created.exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn create_file_uses_unique_destination_and_can_be_undone() {
        let temp = test_dir("create-file");
        fs::create_dir_all(&temp).unwrap();
        fs::write(temp.join("New File.txt"), b"occupied").unwrap();

        let created = create_file(&temp, "New File.txt").unwrap();

        assert_eq!(created.file_name().unwrap(), "New File copy.txt");
        assert!(created.is_file());
        undo_create_file(&created).unwrap();
        assert!(!created.exists());
        assert!(temp.join("New File.txt").exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn write_unique_file_uses_keep_both_names_and_writes_bytes() {
        let temp = test_dir("write-unique-file");
        fs::create_dir_all(&temp).unwrap();
        fs::write(temp.join("Pasted Text.txt"), b"old").unwrap();

        let created = write_unique_file(&temp, "Pasted Text", "txt", b"new").unwrap();

        assert_eq!(created.file_name().unwrap(), "Pasted Text copy.txt");
        assert_eq!(fs::read(&created).unwrap(), b"new");
        assert_eq!(fs::read(temp.join("Pasted Text.txt")).unwrap(), b"old");
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_rename_restores_original_name() {
        let temp = test_dir("undo-rename");
        fs::create_dir_all(&temp).unwrap();
        let original = temp.join("old.txt");
        fs::write(&original, "contents").unwrap();

        let renamed = rename_path(&original, "new.txt").unwrap();
        undo_rename(&original, &renamed).unwrap();

        assert!(original.exists());
        assert!(!renamed.exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[cfg(unix)]
    #[test]
    fn rename_treats_broken_symlink_destination_as_occupied() {
        let temp = test_dir("rename-broken-symlink-conflict");
        fs::create_dir_all(&temp).unwrap();
        let source = temp.join("source.txt");
        let occupied = temp.join("taken.txt");
        fs::write(&source, "contents").unwrap();
        std::os::unix::fs::symlink("missing-target.txt", &occupied).unwrap();

        assert_eq!(
            rename_path(&source, "taken.txt").unwrap_err(),
            "an item with that name already exists"
        );
        assert!(source.exists());
        assert!(!occupied.exists());
        assert!(path_exists(&occupied));

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn undo_trash_restores_original_paths() {
        let temp = test_dir("undo-trash");
        let original_dir = temp.join("originals");
        let trash_dir = temp.join("trash");
        fs::create_dir_all(&original_dir).unwrap();
        fs::create_dir_all(&trash_dir).unwrap();
        let first = original_dir.join("first.txt");
        let second = original_dir.join("second.txt");
        let trashed_first = trash_dir.join("first.txt");
        let trashed_second = trash_dir.join("second.txt");
        fs::write(&trashed_first, "first").unwrap();
        fs::write(&trashed_second, "second").unwrap();

        let items = vec![
            (first.clone(), trashed_first.clone()),
            (second.clone(), trashed_second.clone()),
        ];

        undo_trash(&items).unwrap();

        assert_eq!(fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(fs::read_to_string(&second).unwrap(), "second");
        assert!(!trashed_first.exists());
        assert!(!trashed_second.exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn trash_paths_records_original_and_trash_destinations() {
        let temp = test_dir("trash-records");
        fs::create_dir_all(&temp).unwrap();
        let first = temp.join("first.txt");
        fs::write(&first, "first").unwrap();

        let summary = trash_paths(std::slice::from_ref(&first));

        if summary.failures.is_empty() {
            assert_eq!(summary.successes.len(), 1);
            assert_eq!(summary.successes[0].original_path, first);
            assert!(summary.successes[0].trash_path.exists());
            let _ = undo_trash(&[(
                summary.successes[0].original_path.clone(),
                summary.successes[0].trash_path.clone(),
            )]);
        }
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn trash_paths_async_records_original_and_trash_destinations() {
        let temp = test_dir("trash-records-async");
        fs::create_dir_all(&temp).unwrap();
        let first = temp.join("first.txt");
        fs::write(&first, "first").unwrap();

        let summary =
            futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
                let first = first.clone();
                move || async move { trash_paths_async(vec![first]).await }
            }))
            .unwrap();

        if summary.failures.is_empty() {
            assert_eq!(summary.successes.len(), 1);
            assert_eq!(summary.successes[0].original_path, first);
            assert!(!summary.successes[0].original_path.exists());
            assert!(summary.successes[0].trash_path.exists());
            assert_eq!(
                trash_metadata(&summary.successes[0].trash_path)
                    .unwrap()
                    .original_path,
                summary.successes[0].original_path
            );
            let _ = undo_trash(&[(
                summary.successes[0].original_path.clone(),
                summary.successes[0].trash_path.clone(),
            )]);
        }
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn trash_path_helpers_identify_xdg_trash_files_location() {
        let trash_files = trash_files_dir();

        assert!(is_trash_files_dir(&trash_files));
        assert!(is_in_trash_files_dir(&trash_files.join("trashed.txt")));
        assert!(!is_in_trash_files_dir(
            &trash_files.with_file_name("outside-trash")
        ));
    }

    #[test]
    fn empty_trash_async_removes_files_metadata_and_updates_status() {
        let temp = test_dir("empty-trash-async");
        let files_dir = temp.join("Trash").join("files");
        let info_dir = temp.join("Trash").join("info");
        let trashrc = temp.join("config").join("trashrc");
        fs::create_dir_all(&files_dir).unwrap();
        fs::create_dir_all(&info_dir).unwrap();

        let original = temp.join("original.txt");
        let trash_path = files_dir.join("trashed.txt");
        fs::write(&trash_path, b"trashed").unwrap();
        fs::write(info_dir.join("trashed.txt.trashinfo"), trashinfo(&original)).unwrap();
        fs::write(
            info_dir.join("orphan.trashinfo"),
            trashinfo(&temp.join("orphan.txt")),
        )
        .unwrap();
        write_trash_status_empty_at(&trashrc, false).unwrap();

        let summary =
            futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
                let files_dir = files_dir.clone();
                let info_dir = info_dir.clone();
                let trashrc = trashrc.clone();
                move || async move { empty_trash_in_dirs_async(files_dir, info_dir, trashrc).await }
            }))
            .unwrap();

        assert_eq!(summary.successes.len(), 1);
        assert_eq!(summary.successes[0].original_path, original);
        assert!(summary.failures.is_empty());
        assert!(fs::read_dir(&files_dir).unwrap().next().is_none());
        assert!(fs::read_dir(&info_dir).unwrap().next().is_none());
        assert!(trash_status_empty_at(&trashrc));

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn trashinfo_path_decodes_original_location() {
        let info = "[Trash Info]\nPath=/tmp/a%20b%5Bc%5D.txt\nDeletionDate=2026-06-02T10:11:12\n";

        assert_eq!(
            trash_original_path_from_info(info).unwrap(),
            PathBuf::from("/tmp/a b[c].txt")
        );
    }

    #[test]
    fn trashinfo_path_rejects_missing_relative_or_invalid_values() {
        assert_eq!(
            trash_original_path_from_info("[Trash Info]\nDeletionDate=now\n").unwrap_err(),
            "trash metadata is missing Path"
        );
        assert_eq!(
            trash_original_path_from_info("[Trash Info]\nPath=relative/file.txt\n").unwrap_err(),
            "trash metadata Path is not absolute: relative/file.txt"
        );
        assert_eq!(
            trash_original_path_from_info("[Trash Info]\nPath=/tmp/%XX.txt\n").unwrap_err(),
            "trash metadata Path contains invalid percent escape"
        );
    }

    #[test]
    fn trashrc_status_empty_defaults_and_parses_status_group() {
        assert_eq!(trash_status_empty_from_contents(""), None);
        assert_eq!(
            trash_status_empty_from_contents("[Other]\nEmpty=false\n"),
            None
        );
        assert_eq!(
            trash_status_empty_from_contents("[Status]\nEmpty=false\n"),
            Some(false)
        );
        assert_eq!(
            trash_status_empty_from_contents("[Status]\nEmpty=true\n"),
            Some(true)
        );
        assert_eq!(
            trash_status_empty_from_contents("[Status]\nEmpty=1\n"),
            Some(true)
        );
        assert_eq!(
            trash_status_empty_from_contents("[Status]\nEmpty=no\n"),
            Some(false)
        );
    }

    #[test]
    fn trashrc_status_write_round_trips() {
        let temp = test_dir("trashrc-status");
        let path = temp.join("config").join("trashrc");

        assert!(trash_status_empty_at(&path));

        write_trash_status_empty_at(&path, false).unwrap();
        assert!(!trash_status_empty_at(&path));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[Status]\nEmpty=false\n"
        );

        write_trash_status_empty_at(&path, true).unwrap();
        assert!(trash_status_empty_at(&path));
        assert_eq!(fs::read_to_string(&path).unwrap(), "[Status]\nEmpty=true\n");

        let _ = fs::remove_dir_all(temp);
    }

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fika-file-ops-{name}-{}-{}",
            std::process::id(),
            current_trash_time()
        ))
    }
}
