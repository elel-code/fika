pub(crate) fn status_summary_for_model(
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

pub(crate) fn status_summary_for_model_indexes(
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
