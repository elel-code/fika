use super::{
    CreateUndoItem, CreatedItemKind, TransferUndoItem, TrashUndoItem, UndoPayload, UndoRecord,
};
use crate::core::file_ops;
use crate::core::pane::PaneId;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileTransferMode {
    Copy,
    Move,
    Link,
}

impl FileTransferMode {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Move => "move",
            Self::Link => "link",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Link => "Link",
        }
    }

    pub fn progress_label(self, item_count: usize) -> String {
        let verb = match self {
            Self::Copy => "Copying",
            Self::Move => "Moving",
            Self::Link => "Linking",
        };
        format!("{verb} {item_count} item(s)")
    }
}

#[derive(Clone, Debug)]
pub struct TrashSelectionResult {
    pub pane_id: PaneId,
    pub success_count: usize,
    pub failure_count: usize,
    pub affected_dirs: Vec<PathBuf>,
    pub undo_items: Vec<TrashUndoItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrashViewOperation {
    Restore,
    DeletePermanently,
    Empty,
}

impl TrashViewOperation {
    pub fn progress_label(self, count: usize) -> String {
        match self {
            Self::Restore => format!("Restoring {count} item(s)"),
            Self::DeletePermanently => format!("Deleting {count} item(s) permanently"),
            Self::Empty => "Emptying Trash".to_string(),
        }
    }

    pub fn completed_label(self) -> &'static str {
        match self {
            Self::Restore => "Restored from trash",
            Self::DeletePermanently => "Deleted permanently",
            Self::Empty => "Emptied Trash",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TrashViewOperationResult {
    pub pane_id: PaneId,
    pub operation: TrashViewOperation,
    pub success_count: usize,
    pub failure_count: usize,
    pub affected_dirs: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct TransferTaskResult {
    pub pane_id: PaneId,
    pub mode: FileTransferMode,
    pub label: &'static str,
    pub clear_clipboard: bool,
    pub success_count: usize,
    pub failure_count: usize,
    pub affected_dirs: Vec<PathBuf>,
    pub undo_items: Vec<TransferUndoItem>,
    pub created_items: Vec<CreateUndoItem>,
}

#[derive(Clone, Debug)]
pub struct RenameItemResult {
    pub pane_id: PaneId,
    pub original_path: PathBuf,
    pub affected_dirs: Vec<PathBuf>,
    pub result: Result<PathBuf, String>,
}

#[derive(Clone, Debug)]
pub struct CreateItemResult {
    pub pane_id: PaneId,
    pub kind: CreatedItemKind,
    pub affected_dirs: Vec<PathBuf>,
    pub result: Result<PathBuf, String>,
}

#[derive(Clone, Debug)]
pub struct UndoTaskResult {
    pub record: UndoRecord,
    pub result: Result<String, String>,
}

pub fn rename_item_result(
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

pub fn create_item_result(
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

pub fn default_created_item_name(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "New File.txt",
        CreatedItemKind::Folder => "New Folder",
    }
}

pub fn created_item_label(kind: CreatedItemKind) -> &'static str {
    match kind {
        CreatedItemKind::File => "File",
        CreatedItemKind::Folder => "Folder",
    }
}

pub fn paste_text_result(pane_id: PaneId, target_dir: PathBuf, text: &str) -> TransferTaskResult {
    let mut affected_dirs = Vec::new();
    let mut created_items = Vec::new();
    let result = file_ops::write_unique_file(&target_dir, "Pasted Text", "txt", text.as_bytes());
    let (success_count, failure_count) = match result {
        Ok(path) => {
            push_unique_path(&mut affected_dirs, target_dir);
            created_items.push(CreateUndoItem {
                path,
                kind: CreatedItemKind::File,
            });
            (1, 0)
        }
        Err(_) => (0, 1),
    };

    TransferTaskResult {
        pane_id,
        mode: FileTransferMode::Copy,
        label: "Paste",
        clear_clipboard: false,
        success_count,
        failure_count,
        affected_dirs,
        undo_items: Vec::new(),
        created_items,
    }
}

pub fn transfer_paths_result(
    pane_id: PaneId,
    target_dir: PathBuf,
    mode: FileTransferMode,
    paths: Vec<PathBuf>,
    label: &'static str,
    clear_clipboard: bool,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<Mutex<file_ops::TransferProgress>>>,
) -> TransferTaskResult {
    let operation = mode.operation();
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut affected_dirs = Vec::new();
    let mut undo_items = Vec::new();

    for source in paths {
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
                if mode == FileTransferMode::Move
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

    TransferTaskResult {
        pane_id,
        mode,
        label,
        clear_clipboard,
        success_count,
        failure_count,
        affected_dirs,
        undo_items,
        created_items: Vec::new(),
    }
}

pub fn trash_selection_result(
    pane_id: PaneId,
    selected_paths: Vec<PathBuf>,
) -> TrashSelectionResult {
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

pub fn trash_view_operation_result(
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

pub fn undo_record_result(record: UndoRecord) -> UndoTaskResult {
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

pub fn parent_dirs(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
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

pub fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

pub fn action_status(label: &str, success_count: usize, failure_count: usize) -> String {
    match (success_count, failure_count) {
        (0, 0) => format!("{label}: no changes"),
        (_, 0) => format!("{label}: {success_count} item(s)"),
        (0, _) => format!("{label} failed for {failure_count} item(s)"),
        (_, _) => format!("{label}: {success_count} item(s), {failure_count} failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::operations::{CreateUndoItem, RenameUndoItem, UndoSerial};

    fn test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "fika-operation-tasks-{name}-{}-{nanos}",
            std::process::id()
        ))
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
    fn create_item_result_uses_requested_parent_directory() {
        let temp = test_dir("create-item-parent");
        let current_dir = temp.join("current");
        let directory_target = temp.join("directory-target");
        std::fs::create_dir_all(&current_dir).unwrap();
        std::fs::create_dir_all(&directory_target).unwrap();

        let result = create_item_result(PaneId(7), directory_target.clone(), CreatedItemKind::File);

        assert_eq!(result.result, Ok(directory_target.join("New File.txt")));
        assert!(directory_target.join("New File.txt").exists());
        assert!(!current_dir.join("New File.txt").exists());
        assert_eq!(result.affected_dirs, vec![directory_target]);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn paste_text_result_writes_plain_text_file_and_records_create_undo() {
        let temp = test_dir("paste-text");
        std::fs::create_dir_all(&temp).unwrap();

        let result = paste_text_result(PaneId(15), temp.clone(), "plain text");

        let destination = temp.join("Pasted Text.txt");
        assert_eq!(result.pane_id, PaneId(15));
        assert_eq!(result.mode, FileTransferMode::Copy);
        assert!(!result.clear_clipboard);
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
    fn transfer_paths_result_copies_item_and_records_transfer_undo() {
        let temp = test_dir("transfer-copy");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        std::fs::write(&source, "copy").unwrap();

        let result = transfer_paths_result(
            PaneId(7),
            target_dir.clone(),
            FileTransferMode::Copy,
            vec![source.clone()],
            "Copy",
            false,
            None,
            None,
        );

        let destination = target_dir.join("note.txt");
        assert_eq!(result.pane_id, PaneId(7));
        assert_eq!(result.mode, FileTransferMode::Copy);
        assert!(!result.clear_clipboard);
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
    fn transfer_paths_result_moves_item_and_marks_both_directories() {
        let temp = test_dir("transfer-move");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        std::fs::write(&source, "move").unwrap();

        let result = transfer_paths_result(
            PaneId(8),
            target_dir.clone(),
            FileTransferMode::Move,
            vec![source.clone()],
            "Move",
            true,
            None,
            None,
        );

        let destination = target_dir.join("note.txt");
        assert_eq!(result.mode, FileTransferMode::Move);
        assert!(result.clear_clipboard);
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
    fn transfer_paths_result_updates_shared_transfer_progress() {
        let temp = test_dir("transfer-progress");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.bin");
        std::fs::write(&source, vec![42_u8; 32 * 1024]).unwrap();
        let progress = Arc::new(Mutex::new(file_ops::TransferProgress::default()));

        let result = transfer_paths_result(
            PaneId(13),
            target_dir,
            FileTransferMode::Copy,
            vec![source],
            "Copy",
            false,
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
    fn transfer_paths_result_honors_cancel_flag_before_transfer() {
        let temp = test_dir("transfer-cancel");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.bin");
        std::fs::write(&source, "cancel").unwrap();
        let cancel = Arc::new(AtomicBool::new(true));

        let result = transfer_paths_result(
            PaneId(14),
            target_dir.clone(),
            FileTransferMode::Copy,
            vec![source],
            "Copy",
            false,
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
    fn undo_record_result_restores_transfer_payload() {
        let temp = test_dir("undo-transfer");
        let source_dir = temp.join("source");
        let target_dir = temp.join("target");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        let source = source_dir.join("note.txt");
        let destination = target_dir.join("note.txt");
        std::fs::write(&source, "undo").unwrap();

        let transfer = transfer_paths_result(
            PaneId(9),
            target_dir,
            FileTransferMode::Move,
            vec![source.clone()],
            "Move",
            true,
            None,
            None,
        );
        assert_eq!(transfer.success_count, 1);
        assert!(destination.exists());
        assert!(!source.exists());

        let undo = undo_record_result(UndoRecord {
            serial: UndoSerial(1),
            label: "Move".to_string(),
            affected_dirs: transfer.affected_dirs,
            payload: UndoPayload::Transfer {
                items: transfer.undo_items,
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
            serial: UndoSerial(1),
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
            serial: UndoSerial(1),
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
}
