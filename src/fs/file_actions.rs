use super::{file_ops, privilege};
use crate::app::state::{FileUndo, FileUndoItem};
use crate::desktop::clipboard;
use crate::{AppState, AppWindow, AsyncBridge, AsyncEvent, send_async_event, set_status};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub(crate) struct FileActionResult {
    pub(crate) action: &'static str,
    pub(crate) affected_dir: PathBuf,
    pub(crate) privileged_command: Option<privilege::PrivilegedCommand>,
    pub(crate) result: Result<String, String>,
    pub(crate) undo: Option<FileUndo>,
}

#[derive(Debug)]
pub(crate) struct FileActionApplyResult {
    pub(crate) status: Option<String>,
    pub(crate) undo: Option<FileUndo>,
}

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_create_folder(move |name| {
            if let Some(ui) = ui_weak.upgrade() {
                let parent = state.borrow().panes.active.current_dir.clone();
                let privileged_command = privilege::PrivilegedCommand::CreateFolder {
                    parent: parent.clone(),
                    name: name.to_string(),
                };
                spawn_action(
                    &ui,
                    &bridge,
                    "Create folder",
                    parent.clone(),
                    move || {
                        let path = file_ops::create_folder(&parent, name.as_str())?;
                        Ok((
                            path.display().to_string(),
                            Some(FileUndo {
                                operation: "create-folder".to_string(),
                                original_source: path.clone(),
                                destination: path,
                                overwritten_backup: None,
                                items: Vec::new(),
                            }),
                        ))
                    },
                    Some(privileged_command),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_create_file(move |name| {
            if let Some(ui) = ui_weak.upgrade() {
                let parent = state.borrow().panes.active.current_dir.clone();
                let privileged_command = privilege::PrivilegedCommand::CreateFile {
                    parent: parent.clone(),
                    name: name.to_string(),
                };
                spawn_action(
                    &ui,
                    &bridge,
                    "Create file",
                    parent.clone(),
                    move || {
                        let path = file_ops::create_file(&parent, name.as_str())?;
                        Ok((
                            path.display().to_string(),
                            Some(FileUndo {
                                operation: "create-file".to_string(),
                                original_source: path.clone(),
                                destination: path,
                                overwritten_backup: None,
                                items: Vec::new(),
                            }),
                        ))
                    },
                    Some(privileged_command),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_duplicate_path(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let source = PathBuf::from(path.as_str());
                let Some(parent) = source.parent().map(Path::to_path_buf) else {
                    set_status(&ui, "Cannot duplicate item without a parent folder");
                    return;
                };
                let privileged_command = privilege::PrivilegedCommand::Transfer {
                    operation: "copy".to_string(),
                    source: source.clone(),
                    target_dir: parent.clone(),
                };
                spawn_action(
                    &ui,
                    &bridge,
                    "Duplicate Here",
                    parent.clone(),
                    move || {
                        let destination = file_ops::perform_transfer_with_progress(
                            "copy",
                            &source,
                            &parent,
                            "keep-both",
                            None,
                            |_| {},
                        )?;
                        Ok((
                            destination.display().to_string(),
                            Some(FileUndo {
                                operation: "copy".to_string(),
                                original_source: source,
                                destination,
                                overwritten_backup: None,
                                items: Vec::new(),
                            }),
                        ))
                    },
                    Some(privileged_command),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_copy_location(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let path = path.to_string();
                let affected_dir = PathBuf::from(path.as_str())
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("/"));
                spawn_action(
                    &ui,
                    &bridge,
                    "Copy Location",
                    affected_dir,
                    move || {
                        Ok((
                            clipboard::copy_text(&path)
                                .map(|helper| format!("copied via {helper}"))?,
                            None,
                        ))
                    },
                    None,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_rename_path(move |path, name| {
            if let Some(ui) = ui_weak.upgrade() {
                let path = PathBuf::from(path.as_str());
                let affected_dir = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("/"));
                let privileged_command = privilege::PrivilegedCommand::Rename {
                    path: path.clone(),
                    new_name: name.to_string(),
                };
                spawn_action(
                    &ui,
                    &bridge,
                    "Rename",
                    affected_dir,
                    move || {
                        let original_path = path.clone();
                        let renamed_path = file_ops::rename_path(&path, name.as_str())?;
                        Ok((
                            renamed_path.display().to_string(),
                            (renamed_path != original_path).then_some(FileUndo {
                                operation: "rename".to_string(),
                                original_source: original_path,
                                destination: renamed_path,
                                overwritten_backup: None,
                                items: Vec::new(),
                            }),
                        ))
                    },
                    Some(privileged_command),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_trash_path(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let path = PathBuf::from(path.as_str());
                let affected_dir = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("/"));
                let privileged_command = privilege::PrivilegedCommand::Trash {
                    paths: vec![path.clone()],
                };
                spawn_action(
                    &ui,
                    &bridge,
                    "Move to Trash",
                    affected_dir,
                    move || {
                        let summary = file_ops::trash_paths(&[path]);
                        let undo = trash_undo_from_summary(&summary);
                        Ok((summary.to_result_message("moved to trash")?, undo))
                    },
                    Some(privileged_command),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_trash_selected(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let paths = state
                    .borrow()
                    .panes
                    .active
                    .selection
                    .paths
                    .iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();
                let affected_dir = state.borrow().panes.active.current_dir.clone();
                let privileged_command = privilege::PrivilegedCommand::Trash {
                    paths: paths.clone(),
                };
                spawn_action(
                    &ui,
                    &bridge,
                    "Move selected to Trash",
                    affected_dir,
                    move || {
                        let summary = file_ops::trash_paths(&paths);
                        let undo = trash_undo_from_summary(&summary);
                        Ok((summary.to_result_message("moved to trash")?, undo))
                    },
                    Some(privileged_command),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_restore_trash_path(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let path = PathBuf::from(path.as_str());
                let affected_dir = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(file_ops::trash_files_dir);
                spawn_action(
                    &ui,
                    &bridge,
                    "Restore From Trash",
                    affected_dir,
                    move || {
                        let summary = file_ops::restore_trash_paths(&[path]);
                        Ok((summary.to_result_message("restored from trash")?, None))
                    },
                    None,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_restore_trash_selected(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let paths = state
                    .borrow()
                    .panes
                    .active
                    .selection
                    .paths
                    .iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();
                let affected_dir = state.borrow().panes.active.current_dir.clone();
                spawn_action(
                    &ui,
                    &bridge,
                    "Restore selected from Trash",
                    affected_dir,
                    move || {
                        let summary = file_ops::restore_trash_paths(&paths);
                        Ok((summary.to_result_message("restored from trash")?, None))
                    },
                    None,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_delete_permanently_path(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                let path = PathBuf::from(path.as_str());
                let affected_dir = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(file_ops::trash_files_dir);
                spawn_action(
                    &ui,
                    &bridge,
                    "Delete Permanently",
                    affected_dir,
                    move || {
                        let summary = file_ops::permanently_delete_trash_paths(&[path]);
                        Ok((summary.to_result_message("deleted permanently")?, None))
                    },
                    None,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_delete_permanently_selected(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let paths = state
                    .borrow()
                    .panes
                    .active
                    .selection
                    .paths
                    .iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();
                let affected_dir = state.borrow().panes.active.current_dir.clone();
                spawn_action(
                    &ui,
                    &bridge,
                    "Delete selected permanently",
                    affected_dir,
                    move || {
                        let summary = file_ops::permanently_delete_trash_paths(&paths);
                        Ok((summary.to_result_message("deleted permanently")?, None))
                    },
                    None,
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_empty_trash(move || {
            if let Some(ui) = ui_weak.upgrade() {
                spawn_action(
                    &ui,
                    &bridge,
                    "Empty Trash",
                    file_ops::trash_files_dir(),
                    move || {
                        let summary = file_ops::empty_trash();
                        Ok((summary.to_result_message("removed from Trash")?, None))
                    },
                    None,
                );
            }
        });
    }
}

fn trash_undo_from_summary(summary: &file_ops::FileActionSummary) -> Option<FileUndo> {
    let first = summary.successes.first()?;
    Some(FileUndo {
        operation: "trash".to_string(),
        original_source: first.original_path.clone(),
        destination: first.trash_path.clone(),
        overwritten_backup: None,
        items: summary
            .successes
            .iter()
            .map(|record| FileUndoItem {
                original_source: record.original_path.clone(),
                destination: record.trash_path.clone(),
            })
            .collect(),
    })
}

pub(crate) fn apply_file_action_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    result: FileActionResult,
) -> FileActionApplyResult {
    state
        .borrow_mut()
        .remove_directory_cache(&result.affected_dir);
    match result.result {
        Ok(message) => FileActionApplyResult {
            status: Some(format!("{} complete: {message}", result.action)),
            undo: result.undo,
        },
        Err(err) if privilege::is_permission_error(&err) => {
            if let Some(command) = result.privileged_command {
                request_privileged_action(ui, state, command, &err);
                FileActionApplyResult {
                    status: None,
                    undo: None,
                }
            } else {
                FileActionApplyResult {
                    status: Some(format!("{} failed: {err}", result.action)),
                    undo: None,
                }
            }
        }
        Err(err) => FileActionApplyResult {
            status: Some(format!("{} failed: {err}", result.action)),
            undo: None,
        },
    }
}

fn spawn_action(
    ui: &AppWindow,
    bridge: &AsyncBridge,
    action: &'static str,
    affected_dir: PathBuf,
    task: impl FnOnce() -> Result<(String, Option<FileUndo>), String> + Send + 'static,
    privileged_command: Option<privilege::PrivilegedCommand>,
) {
    set_status(ui, &format!("{action}..."));
    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = tokio::task::spawn_blocking(task)
            .await
            .unwrap_or_else(|err| Err(format!("file action task failed: {err}")));
        let (result, undo) = match result {
            Ok((message, undo)) => (Ok(message), undo),
            Err(err) => (Err(err), None),
        };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileActionFinished(FileActionResult {
                action,
                affected_dir,
                privileged_command,
                result,
                undo,
            }),
        );
    });
}

pub(crate) fn request_privileged_action(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    command: privilege::PrivilegedCommand,
    reason: &str,
) {
    ui.set_privileged_prompt_title(
        format!("{} requires administrator privileges", command.label()).into(),
    );
    ui.set_privileged_prompt_message(command.summary().into());
    ui.set_privileged_prompt_detail(reason.into());
    ui.set_privileged_prompt_open(true);
    state.borrow_mut().pending_privileged_command = Some(command);
}

trait SummaryMessage {
    fn to_result_message(self, success_label: &str) -> Result<String, String>;
}

impl SummaryMessage for file_ops::FileActionSummary {
    fn to_result_message(self, success_label: &str) -> Result<String, String> {
        match (self.successes.len(), self.failures.is_empty()) {
            (0, false) => Err(self.failures.join("; ")),
            (count, true) => Ok(format!("{count} item(s) {success_label}")),
            (count, false) => Ok(format!(
                "{count} item(s) {success_label}; {} failure(s): {}",
                self.failures.len(),
                self.failures.join("; ")
            )),
        }
    }
}
