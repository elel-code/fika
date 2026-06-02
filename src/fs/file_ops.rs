use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default)]
pub struct TransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Clone, Debug)]
pub struct TransferOutcome {
    pub destination: PathBuf,
    pub overwritten_backup: Option<PathBuf>,
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
    cancel: Option<Arc<AtomicBool>>,
    progress: impl FnMut(TransferProgress),
) -> Result<PathBuf, String> {
    let outcome = perform_transfer_with_progress_outcome(
        operation,
        source,
        target_dir,
        conflict_policy,
        cancel,
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
    cancel: Option<Arc<AtomicBool>>,
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
        "move" => move_path(source, &destination, cancel.as_ref(), &mut progress),
        "copy" => copy_path(source, &destination, cancel.as_ref(), &mut progress),
        "link" => link_path(source, &destination),
        _ => unreachable!("operation was validated before dispatch"),
    };

    match result {
        Ok(()) => {
            // Keep the backup in the richer outcome so callers that expose Undo can restore it.
            // The compatibility wrapper removes it immediately for one-shot operations.
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
    Ok(destination)
}

pub fn ensure_trash_dirs() -> Result<(), String> {
    fs::create_dir_all(trash_files_dir()).map_err(|err| err.to_string())?;
    fs::create_dir_all(trash_info_dir()).map_err(|err| err.to_string())
}

pub fn trash_files_dir() -> PathBuf {
    trash_home().join("files")
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
        fs::rename(trash_path, original_path).map_err(|err| err.to_string())?;
        let _ = remove_trashinfo(trash_path);
    }

    Ok(format!("restored {} item(s)", items.len()))
}

pub fn restore_trash_paths(paths: &[PathBuf]) -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    for path in paths {
        match restore_trash_path(path) {
            Ok(record) => summary.successes.push(record),
            Err(err) => summary.failures.push(format!("{}: {err}", path.display())),
        }
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
    summary
}

pub fn empty_trash() -> FileActionSummary {
    let mut summary = FileActionSummary::default();
    let files_dir = trash_files_dir();
    if !path_exists(&files_dir) {
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
    summary
}

fn restore_trash_path(trash_path: &Path) -> Result<TrashRecord, String> {
    if is_trash_files_dir(trash_path) || !is_in_trash_files_dir(trash_path) {
        return Err("item is not inside Trash".to_string());
    }
    if !path_exists(trash_path) {
        return Err("trash item no longer exists".to_string());
    }

    let original_path = trash_original_path(trash_path)?;
    if path_exists(&original_path) {
        return Err(format!(
            "original location is already occupied: {}",
            original_path.display()
        ));
    }

    if let Some(parent) = original_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::rename(trash_path, &original_path).map_err(|err| err.to_string())?;
    let _ = remove_trashinfo(trash_path);

    Ok(TrashRecord {
        original_path,
        trash_path: trash_path.to_path_buf(),
    })
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

#[derive(Debug, Default)]
pub struct FileActionSummary {
    pub successes: Vec<TrashRecord>,
    pub failures: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TrashRecord {
    pub original_path: PathBuf,
    pub trash_path: PathBuf,
}

fn move_path(
    source: &Path,
    destination: &Path,
    cancel: Option<&Arc<AtomicBool>>,
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
    cancel: Option<&Arc<AtomicBool>>,
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
    cancel: Option<&Arc<AtomicBool>>,
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
    cancel: Option<&Arc<AtomicBool>>,
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
    cancel: Option<&Arc<AtomicBool>>,
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
    cancel: Option<&Arc<AtomicBool>>,
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

fn ensure_not_cancelled(cancel: Option<&Arc<AtomicBool>>) -> io::Result<()> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
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

fn trash_original_path(trash_path: &Path) -> Result<PathBuf, String> {
    let info_path =
        trash_info_path(trash_path).ok_or_else(|| "trash item has no metadata name".to_string())?;
    let contents = fs::read_to_string(&info_path).map_err(|err| {
        format!(
            "failed to read trash metadata {}: {err}",
            info_path.display()
        )
    })?;
    trash_original_path_from_info(&contents)
}

fn trash_original_path_from_info(contents: &str) -> Result<PathBuf, String> {
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
    Ok(path)
}

fn trash_info_path(trash_path: &Path) -> Option<PathBuf> {
    let name = trash_path.file_name()?.to_str()?;
    Some(trash_info_dir().join(format!("{name}.trashinfo")))
}

fn trash_info_dir() -> PathBuf {
    trash_home().join("info")
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
    use std::sync::atomic::AtomicBool;

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
    fn copy_can_be_cancelled() {
        let temp = test_dir("cancel");
        fs::create_dir_all(&temp).unwrap();
        let source = temp.join("source.bin");
        let target = temp.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(&source, vec![11_u8; 256 * 1024]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_from_progress = Arc::clone(&cancel);
        let result = perform_transfer_with_progress(
            "copy",
            &source,
            &target,
            "keep-both",
            Some(cancel),
            move |_| cancel_from_progress.store(true, Ordering::Relaxed),
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

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_from_progress = Arc::clone(&cancel);
        let result = perform_transfer_with_progress(
            "copy",
            &source,
            &target,
            "keep-both",
            Some(cancel),
            move |_| cancel_from_progress.store(true, Ordering::Relaxed),
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
    fn trash_path_helpers_identify_xdg_trash_files_location() {
        let trash_files = trash_files_dir();

        assert!(is_trash_files_dir(&trash_files));
        assert!(is_in_trash_files_dir(&trash_files.join("trashed.txt")));
        assert!(!is_in_trash_files_dir(
            &trash_files.with_file_name("outside-trash")
        ));
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

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fika-file-ops-{name}-{}-{}",
            std::process::id(),
            current_trash_time()
        ))
    }
}
