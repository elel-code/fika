use fika_core::{file_ops, run_operation_task};

use super::{CreateEntryKind, CreateEntryRequest, RenameEntryRequest};

pub(crate) fn create_entry_on_disk(request: &CreateEntryRequest) -> Result<(), String> {
    let path = request.path.clone();
    let kind = request.kind;
    pollster::block_on(run_operation_task(move || async move {
        match kind {
            CreateEntryKind::Folder => file_ops::create_folder_at_async(&path)
                .await
                .map_err(|error| format!("create folder {}: {error}", path.display())),
            CreateEntryKind::File => file_ops::create_file_at_async(&path)
                .await
                .map_err(|error| format!("create file {}: {error}", path.display())),
        }
    }))
    .map_err(|error| error.to_string())?
}

pub(crate) fn rename_entry_on_disk(request: &RenameEntryRequest) -> Result<(), String> {
    let source = request.source.clone();
    let target = request.target.clone();
    pollster::block_on(run_operation_task(move || async move {
        file_ops::rename_path_to_async(&source, &target)
            .await
            .map_err(|error| {
                format!(
                    "rename {} to {}: {error}",
                    source.display(),
                    target.display()
                )
            })
    }))
    .map_err(|error| error.to_string())?
}
