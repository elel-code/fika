use super::file_ops;
use std::cmp::Reverse;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub group: String,
    pub location: String,
    pub kind: String,
    pub size: String,
    pub size_bytes: u64,
    pub modified: String,
    pub modified_age_days: i32,
    pub is_dir: bool,
}

impl Entry {
    pub fn sort_key(&self) -> (u8, String) {
        (u8::from(!self.is_dir), self.name.to_ascii_lowercase())
    }
}

pub fn read_entries_sync(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let decorate_trash_metadata = file_ops::is_trash_files_dir(path);

    for item in std::fs::read_dir(path)? {
        let Ok(item) = item else {
            continue;
        };
        if let Ok(metadata) = item.metadata() {
            let item_path = item.path();
            let name = item.file_name().to_string_lossy().trim().to_string();
            if name.is_empty() {
                continue;
            }
            let mut entry = to_entry(item_path.clone(), name, String::new(), metadata);
            if decorate_trash_metadata {
                decorate_trash_entry(&mut entry, &item_path);
            }
            entries.push(entry);
        }
    }

    sort_entries(&mut entries, decorate_trash_metadata);
    Ok(entries)
}

pub fn read_entry_sync(directory: &Path, path: &Path) -> io::Result<Entry> {
    let decorate_trash_metadata = file_ops::is_trash_files_dir(directory);
    let item_path = directory_entry_path(directory, path).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "directory item is outside directory",
        )
    })?;
    let metadata = std::fs::metadata(&item_path)?;
    let name = item_path
        .file_name()
        .map(|name| name.to_string_lossy().trim().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "directory item has no name"))?;
    let mut entry = to_entry(item_path.clone(), name, String::new(), metadata);
    if decorate_trash_metadata {
        decorate_trash_entry(&mut entry, &item_path);
    }
    Ok(entry)
}

pub fn sort_entries(entries: &mut [Entry], trash: bool) {
    if trash {
        entries.sort_by_cached_key(trash_sort_key);
    } else {
        entries.sort_by_cached_key(Entry::sort_key);
    }
}

pub fn directory_entry_path(directory: &Path, path: &Path) -> Option<PathBuf> {
    if file_ops::is_trash_files_dir(directory)
        && path
            .parent()
            .is_some_and(|parent| parent == file_ops::trash_info_dir())
        && path.extension().and_then(|extension| extension.to_str()) == Some("trashinfo")
    {
        let stem = path.file_stem()?;
        return Some(file_ops::trash_files_dir().join(stem));
    }

    path.parent()
        .is_some_and(|parent| parent == directory)
        .then(|| path.to_path_buf())
}

fn decorate_trash_entry(entry: &mut Entry, path: &Path) {
    let Ok(metadata) = file_ops::trash_metadata(path) else {
        return;
    };
    entry.group = trash_group_label(&metadata.original_path, metadata.deletion_date.as_deref());
    if let Some(deletion_date) = metadata.deletion_date {
        entry.modified = format_trash_deletion_date(&deletion_date);
        entry.modified_age_days = -1;
    }
}

fn trash_group_label(original_path: &Path, deletion_date: Option<&str>) -> String {
    let original_location = original_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(original_path);
    match deletion_date {
        Some(date) => format!(
            "Original: {} - Deleted: {}",
            original_location.display(),
            format_trash_deletion_date(date)
        ),
        None => format!("Original: {}", original_location.display()),
    }
}

fn format_trash_deletion_date(value: &str) -> String {
    let normalized = value.replace('T', " ");
    normalized
        .strip_suffix(":00")
        .unwrap_or(&normalized)
        .to_string()
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn trash_sort_key(entry: &Entry) -> (u8, Reverse<String>, String) {
    let bucket = trash_sort_bucket(entry);
    let deletion_date = if bucket == 0 {
        Reverse(entry.modified.clone())
    } else {
        Reverse(String::new())
    };
    (bucket, deletion_date, entry.name.to_ascii_lowercase())
}

fn trash_sort_bucket(entry: &Entry) -> u8 {
    if entry.group.contains("Deleted: ") {
        0
    } else if !entry.group.is_empty() {
        1
    } else {
        2
    }
}

fn to_entry(path: PathBuf, name: String, location: String, metadata: Metadata) -> Entry {
    let is_dir = metadata.is_dir();
    let size_bytes = if is_dir { 0 } else { metadata.len() };
    let modified = metadata.modified().ok();

    Entry {
        name,
        path,
        group: String::new(),
        location,
        kind: if is_dir { "Folder" } else { "File" }.to_string(),
        size: if is_dir {
            "-".to_string()
        } else {
            format_size(size_bytes)
        },
        size_bytes,
        modified: modified
            .map(format_system_time)
            .unwrap_or_else(|| "-".to_string()),
        modified_age_days: modified.map(modified_age_days).unwrap_or(-1),
        is_dir,
    }
}

fn modified_age_days(time: SystemTime) -> i32 {
    match SystemTime::now().duration_since(time) {
        Ok(duration) => (duration.as_secs() / 86_400).min(i32::MAX as u64) as i32,
        Err(_) => 0,
    }
}

fn format_system_time(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format_unix_time(secs)
}

fn format_unix_time(secs: u64) -> String {
    let days = secs / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let seconds_of_day = secs % 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_file_sizes_without_ui_types() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1536), "1.5 KB");
    }
}
