use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use fika_core::{PaneId, TransferTaskResult, file_ops, paste_text_result, transfer_paths_result};

use super::state::{ClipboardMode, ClipboardState};

pub(crate) fn paste_clipboard_result(
    pane_id: PaneId,
    target_dir: PathBuf,
    clipboard: ClipboardState,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<Mutex<file_ops::TransferProgress>>>,
) -> TransferTaskResult {
    let clipboard_mode = clipboard.mode;
    if let Some(text) = clipboard.text.as_deref() {
        return paste_text_result(pane_id, target_dir, text);
    }
    let label = clipboard.action_label();

    transfer_paths_result(
        pane_id,
        target_dir,
        clipboard_mode.transfer_mode(),
        clipboard.paths,
        label,
        clipboard_mode == ClipboardMode::Cut,
        cancel,
        progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use fika_core::{CreateUndoItem, CreatedItemKind, FileTransferMode, TransferUndoItem};
    use std::time::{SystemTime, UNIX_EPOCH};

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

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fika-clipboard-{name}-{}-{nanos}",
            std::process::id()
        ));
        if path.exists() {
            let _ = std::fs::remove_dir_all(&path);
        }
        path
    }
}
