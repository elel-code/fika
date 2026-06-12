use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::{PropertiesDialogState, PropertyRow};

pub(crate) fn properties_for_path(path: &Path) -> PropertiesDialogState {
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

pub(crate) fn properties_for_selection(paths: &[PathBuf]) -> PropertiesDialogState {
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
